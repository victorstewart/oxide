use std::path::Path;

use oxide_image_sampling_diagnostic as diagnostic;

fn main()
{
   let mut arguments = std::env::args_os().skip(1);
   let Some(source) = arguments.next() else
   {
      eprintln!("usage: oxide-image-sampling-diagnostic <source.png> <output-directory>");
      std::process::exit(2);
   };
   let Some(output) = arguments.next() else
   {
      eprintln!("usage: oxide-image-sampling-diagnostic <source.png> <output-directory>");
      std::process::exit(2);
   };
   if arguments.next().is_some()
   {
      eprintln!("usage: oxide-image-sampling-diagnostic <source.png> <output-directory>");
      std::process::exit(2);
   }
   match diagnostic::run(Path::new(&source), Path::new(&output))
   {
      Ok(summary) =>
      {
         println!("{}", summary.report_path.display());
      }
      Err(error) =>
      {
         eprintln!("image sampling diagnostic failed: {error}");
         std::process::exit(1);
      }
   }
}
