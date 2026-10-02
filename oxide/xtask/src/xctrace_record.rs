use std::fs;
use std::io::ErrorKind;
use std::ops::{Deref, DerefMut};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{bail, Context, Result};

pub const XCTRACE_RECORD_WORKING_SET_LIMIT_BYTES: u64 = 512 * 1024 * 1024;

const XCTRACE_FUSE_POLL_MS: u64 = 100;
const FUSE_OK: u8 = 0;
const FUSE_LIMIT_EXCEEDED: u8 = 1;
const FUSE_MEASUREMENT_FAILED: u8 = 2;

static XCTRACE_SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct XctraceScratch
{
   path: PathBuf,
}

impl XctraceScratch
{
   fn create(trace_path: &Path) -> Result<Self>
   {
      let parent = trace_path
         .parent()
         .with_context(|| format!("xctrace output has no parent: {}", trace_path.display()))?;
      let stem = trace_path
         .file_name()
         .and_then(|value| value.to_str())
         .unwrap_or("trace");
      for _ in 0..32
      {
         let sequence = XCTRACE_SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
         let path = parent.join(format!(
            ".{}.xctrace-tmp-{}-{}",
            stem,
            std::process::id(),
            sequence
         ));
         match fs::create_dir(&path)
         {
            Ok(()) => return Ok(Self {path}),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) =>
            {
               return Err(error)
                  .with_context(|| format!("creating isolated xctrace scratch directory {}", path.display()));
            }
         }
      }
      bail!("could not allocate a unique xctrace scratch directory beside {}", trace_path.display())
   }

   fn cleanup(&mut self) -> Result<()>
   {
      remove_path_if_present(&self.path)
         .with_context(|| format!("removing isolated xctrace scratch directory {}", self.path.display()))
   }
}

impl Drop for XctraceScratch
{
   fn drop(&mut self)
   {
      let _ = self.cleanup();
   }
}

pub struct XctraceRecordProcess
{
   child: Child,
   process_group: u32,
   trace_path: PathBuf,
   scratch: XctraceScratch,
   limit_bytes: u64,
   stop_monitor: Arc<AtomicBool>,
   fuse_state: Arc<AtomicU8>,
   monitor: Option<JoinHandle<()>>,
   process_group_active: bool,
   retain_trace: bool,
}

impl XctraceRecordProcess
{
   pub fn spawn(root: &Path, program: &str, args: &[String], trace_path: &Path, stdout_path: &Path, stderr_path: &Path, limit_bytes: u64) -> Result<Self>
   {
      if limit_bytes == 0
      {
         bail!("xctrace working-set limit must be positive");
      }
      let scratch = XctraceScratch::create(trace_path)?;
      let stdout_file = fs::File::create(stdout_path)
         .with_context(|| format!("creating {}", stdout_path.display()))?;
      let stderr_file = fs::File::create(stderr_path)
         .with_context(|| format!("creating {}", stderr_path.display()))?;
      println!("> {} {}", program, args.join(" "));
      let child = Command::new(program)
         .args(args)
         .current_dir(root)
         .env("TMPDIR", &scratch.path)
         .env("TMP", &scratch.path)
         .env("TEMP", &scratch.path)
         .process_group(0)
         .stdout(Stdio::from(stdout_file))
         .stderr(Stdio::from(stderr_file))
         .spawn()
         .with_context(|| format!("running {} {}", program, args.join(" ")))?;
      let stop_monitor = Arc::new(AtomicBool::new(false));
      let fuse_state = Arc::new(AtomicU8::new(FUSE_OK));
      let process_group = child.id();
      let monitor = Some(spawn_fuse_monitor(
         process_group,
         trace_path.to_path_buf(),
         scratch.path.clone(),
         limit_bytes,
         Arc::clone(&stop_monitor),
         Arc::clone(&fuse_state),
      ));
      Ok(Self {
         child,
         process_group,
         trace_path: trace_path.to_path_buf(),
         scratch,
         limit_bytes,
         stop_monitor,
         fuse_state,
         monitor,
         process_group_active: true,
         retain_trace: false,
      })
   }

   pub fn scratch_path(&self) -> &Path
   {
      &self.scratch.path
   }

   pub fn working_set_bytes(&self) -> Result<u64>
   {
      trace_working_set_bytes(&self.trace_path, &self.scratch.path)
   }

   pub fn enforce_working_set_limit(&mut self) -> Result<()>
   {
      self.check_fuse()?;
      match self.working_set_bytes()
      {
         Ok(bytes) if bytes <= self.limit_bytes => Ok(()),
         Ok(bytes) =>
         {
            self.trip_fuse(FUSE_LIMIT_EXCEEDED);
            bail!(
               "xctrace record working set reached {} bytes, exceeding its {} byte bundle-plus-scratch limit",
               bytes,
               self.limit_bytes
            )
         }
         Err(error) =>
         {
            self.trip_fuse(FUSE_MEASUREMENT_FAILED);
            Err(error).context("measuring xctrace bundle-plus-scratch working set")
         }
      }
   }

   pub fn check_fuse(&self) -> Result<()>
   {
      match self.fuse_state.load(Ordering::Acquire)
      {
         FUSE_OK => Ok(()),
         FUSE_LIMIT_EXCEEDED => bail!(
            "xctrace record exceeded its {} byte bundle-plus-scratch working-set limit",
            self.limit_bytes
         ),
         FUSE_MEASUREMENT_FAILED => bail!(
            "xctrace record was terminated because its bundle-plus-scratch working set could not be measured safely"
         ),
         state => bail!("xctrace record entered unknown storage-fuse state {}", state),
      }
   }

   pub fn try_wait_checked(&mut self) -> Result<Option<ExitStatus>>
   {
      self.check_fuse()?;
      let status = self.child.try_wait().context("probing xctrace record process")?;
      self.check_fuse()?;
      Ok(status)
   }

   pub fn cleanup_scratch(&mut self) -> Result<()>
   {
      self.enforce_working_set_limit()?;
      if self.child.try_wait().context("probing completed xctrace record process")?.is_none()
      {
         bail!("cannot remove xctrace scratch while its process is still running");
      }
      self.stop_monitor();
      self.check_fuse()?;
      kill_process_group(self.process_group);
      self.process_group_active = false;
      self.scratch.cleanup()
   }

   pub fn commit(&mut self) -> Result<()>
   {
      self.enforce_working_set_limit()?;
      if self.child.try_wait().context("probing completed xctrace record process")?.is_none()
      {
         bail!("cannot commit an xctrace record while its process is still running");
      }
      self.stop_monitor();
      self.check_fuse()?;
      if self.process_group_active
      {
         kill_process_group(self.process_group);
         self.process_group_active = false;
      }
      self.scratch.cleanup()?;
      self.retain_trace = true;
      Ok(())
   }

   fn trip_fuse(&mut self, state: u8)
   {
      let _ = self.fuse_state.compare_exchange(FUSE_OK, state, Ordering::AcqRel, Ordering::Acquire);
      terminate_process_group(&mut self.child, self.process_group);
   }

   fn stop_monitor(&mut self)
   {
      self.stop_monitor.store(true, Ordering::Release);
      if let Some(monitor) = self.monitor.take()
      {
         let _ = monitor.join();
      }
   }
}

impl Deref for XctraceRecordProcess
{
   type Target = Child;

   fn deref(&self) -> &Self::Target
   {
      &self.child
   }
}

impl DerefMut for XctraceRecordProcess
{
   fn deref_mut(&mut self) -> &mut Self::Target
   {
      &mut self.child
   }
}

impl Drop for XctraceRecordProcess
{
   fn drop(&mut self)
   {
      if self.process_group_active
      {
         terminate_process_group(&mut self.child, self.process_group);
      }
      self.stop_monitor();
      let _ = self.scratch.cleanup();
      if !self.retain_trace
      {
         let _ = remove_path_if_present(&self.trace_path);
      }
   }
}

pub fn trace_working_set_bytes(trace_path: &Path, scratch_path: &Path) -> Result<u64>
{
   let trace_bytes = path_bytes(trace_path)?;
   let scratch_bytes = path_bytes(scratch_path)?;
   trace_bytes.checked_add(scratch_bytes).context("xctrace bundle-plus-scratch byte count overflow")
}

fn spawn_fuse_monitor(process_group: u32, trace_path: PathBuf, scratch_path: PathBuf, limit_bytes: u64, stop: Arc<AtomicBool>, fuse_state: Arc<AtomicU8>) -> JoinHandle<()>
{
   thread::spawn(move ||
   {
      while !stop.load(Ordering::Acquire)
      {
         let state = match trace_working_set_bytes(&trace_path, &scratch_path)
         {
            Ok(bytes) if bytes <= limit_bytes => FUSE_OK,
            Ok(_) => FUSE_LIMIT_EXCEEDED,
            Err(_) => FUSE_MEASUREMENT_FAILED,
         };
         if state != FUSE_OK
         {
            let _ = fuse_state.compare_exchange(FUSE_OK, state, Ordering::AcqRel, Ordering::Acquire);
            kill_process_group(process_group);
            return;
         }
         thread::sleep(Duration::from_millis(XCTRACE_FUSE_POLL_MS));
      }
   })
}

fn terminate_process_group(child: &mut Child, process_group: u32)
{
   kill_process_group(process_group);
   if child.try_wait().ok().flatten().is_none()
   {
      let _ = child.kill();
      let _ = child.wait();
   }
}

fn kill_process_group(process_group: u32)
{
   let _ = Command::new("/bin/kill")
      .arg("-KILL")
      .arg(format!("-{}", process_group))
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status();
}

fn path_bytes(path: &Path) -> Result<u64>
{
   let metadata = match fs::symlink_metadata(path)
   {
      Ok(metadata) => metadata,
      Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
      Err(error) => return Err(error).with_context(|| format!("reading metadata for {}", path.display())),
   };
   if metadata.file_type().is_symlink() || metadata.is_file()
   {
      return Ok(metadata.len());
   }
   if !metadata.is_dir()
   {
      return Ok(0);
   }
   let mut total = 0_u64;
   for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?
   {
      let entry = match entry
      {
         Ok(entry) => entry,
         Err(error) if error.kind() == ErrorKind::NotFound => continue,
         Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
      };
      total = total
         .checked_add(path_bytes(&entry.path())?)
         .context("xctrace path byte count overflow")?;
   }
   Ok(total)
}

fn remove_path_if_present(path: &Path) -> Result<()>
{
   let metadata = match fs::symlink_metadata(path)
   {
      Ok(metadata) => metadata,
      Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
      Err(error) => return Err(error.into()),
   };
   if metadata.is_dir() && !metadata.file_type().is_symlink()
   {
      fs::remove_dir_all(path)?;
   }
   else
   {
      fs::remove_file(path)?;
   }
   Ok(())
}
