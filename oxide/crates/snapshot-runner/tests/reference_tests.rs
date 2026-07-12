use oxide_snapshot_runner::reference::{
   asymmetric_id_mask_fixture, id_mask_field_rgba, id_mask_fields_rgba, id_mask_jump_fields,
   id_mask_jump_schedule, id_mask_seed_fields, IdMaskFieldSeed,
};
use std::fs;
use std::fmt::Write;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

fn write_png(path: &Path, width: usize, height: usize, rgba: &[u8])
{
   let file = fs::File::create(path).expect("create reference PNG");
   let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
   encoder.set_color(png::ColorType::Rgba);
   encoder.set_depth(png::BitDepth::Eight);
   encoder
      .write_header()
      .expect("write reference PNG header")
      .write_image_data(rgba)
      .expect("write reference PNG pixels");
}

fn read_png(path: &Path) -> (usize, usize, Vec<u8>)
{
   let bytes = fs::read(path).expect("read reference PNG");
   let mut reader = png::Decoder::new(&bytes[..]).read_info().expect("decode reference PNG");
   let mut rgba = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut rgba).expect("read reference PNG frame");
   rgba.truncate(info.buffer_size());
   assert_eq!(info.color_type, png::ColorType::Rgba);
   (info.width as usize, info.height as usize, rgba)
}

fn golden_dir() -> PathBuf
{
   PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join("goldens/reference")
}

fn write_cpu_reference_json(
   path: &Path,
   width: usize,
   height: usize,
   city: &[u8],
   neighborhood: &[u8],
   fields: &oxide_snapshot_runner::reference::IdMaskFields,
)
{
   let mut json = String::with_capacity(city.len() * 48);
   let _ = write!(json, "{{\n  \"width\": {width},\n  \"height\": {height},\n  \"city\": [");
   write_u8_json(&mut json, city);
   json.push_str("],\n  \"neighborhood\": [");
   write_u8_json(&mut json, neighborhood);
   json.push_str("],\n  \"city_field\": [");
   write_seed_json(&mut json, &fields.city);
   json.push_str("],\n  \"seam_field\": [");
   write_seed_json(&mut json, &fields.seam);
   json.push_str("]\n}\n");
   if let Some(parent) = path.parent()
   {
      fs::create_dir_all(parent).expect("create CPU reference evidence directory");
   }
   fs::write(path, json).expect("write CPU reference JSON");
}

fn write_u8_json(json: &mut String, values: &[u8])
{
   for (index, value) in values.iter().enumerate()
   {
      if index != 0
      {
         json.push(',');
      }
      let _ = write!(json, "{value}");
   }
}

fn write_seed_json(json: &mut String, seeds: &[IdMaskFieldSeed])
{
   for (index, seed) in seeds.iter().enumerate()
   {
      if index != 0
      {
         json.push(',');
      }
      let _ = write!(
         json,
         "[{},{},{},{}]",
         seed.x, seed.y, seed.city, seed.neighborhood,
      );
   }
}

#[test]
fn asymmetric_id_mask_seed_coordinates_and_values_are_exact()
{
   let (width, height, city, neighborhood) = asymmetric_id_mask_fixture();
   let fields = id_mask_seed_fields(width, height, &city, &neighborhood);

   assert_eq!(fields.city[5 * width], IdMaskFieldSeed { x: 0, y: 5, city: 1, neighborhood: 3 });
   assert_eq!(fields.city[5 * width + 1], IdMaskFieldSeed { x: 1, y: 5, city: 1, neighborhood: 7 });
   assert_eq!(fields.seam[5 * width], IdMaskFieldSeed { x: 0, y: 5, city: 1, neighborhood: 1 });
   assert_eq!(fields.seam[5 * width + 1], IdMaskFieldSeed { x: 1, y: 5, city: 1, neighborhood: 1 });
   assert_eq!(fields.city.iter().filter(|seed| seed.valid()).count(), 4);
   assert_eq!(fields.seam.iter().filter(|seed| seed.valid()).count(), 2);
}

#[test]
fn every_asymmetric_id_mask_jfa_jump_changes_the_reference_fields()
{
   let (width, height, city, neighborhood) = asymmetric_id_mask_fixture();
   let jumps = id_mask_jump_schedule(width, height);
   assert_eq!(jumps, [16, 8, 4, 2, 1]);
   let mut fields = id_mask_seed_fields(width, height, &city, &neighborhood);
   for jump in jumps
   {
      let next = id_mask_jump_fields(&fields, jump);
      assert_ne!(next, fields, "jump {jump} did not change the asymmetric field");
      fields = next;
   }
   assert!(fields.city.iter().all(|seed| seed.valid()));
   assert!(fields.seam.iter().all(|seed| seed.valid()));
}

#[test]
fn id_mask_field_reference_image_encodes_seed_coordinates_and_ids()
{
   let (width, height, city, neighborhood) = asymmetric_id_mask_fixture();
   let mut fields = id_mask_seed_fields(width, height, &city, &neighborhood);
   for jump in id_mask_jump_schedule(width, height)
   {
      fields = id_mask_jump_fields(&fields, jump);
   }
   let rgba = id_mask_field_rgba(width, height, &fields.city);

   assert_eq!(rgba.len(), width * height * 4);
   assert!(rgba.chunks_exact(4).all(|pixel| pixel[3] != 0));
   assert!(rgba.chunks_exact(4).any(|pixel| pixel[2] == 1));
   assert!(rgba.chunks_exact(4).any(|pixel| pixel[2] == 2));
   assert!(rgba.chunks_exact(4).any(|pixel| pixel[2] == 3));
   if let Some(path) = std::env::var_os("OXIDE_ID_MASK_REFERENCE_JSON")
   {
      write_cpu_reference_json(
         Path::new(&path),
         width,
         height,
         &city,
         &neighborhood,
         &fields,
      );
   }
}

#[test]
fn committed_asymmetric_id_mask_stage_images_are_exact()
{
   let (width, height, city, neighborhood) = asymmetric_id_mask_fixture();
   let mut fields = id_mask_seed_fields(width, height, &city, &neighborhood);
   let mut stages = vec![("id_mask_seed.png", id_mask_fields_rgba(&fields))];
   for jump in id_mask_jump_schedule(width, height)
   {
      fields = id_mask_jump_fields(&fields, jump);
      stages.push((
         match jump
         {
            16 => "id_mask_jump16.png",
            8 => "id_mask_jump8.png",
            4 => "id_mask_jump4.png",
            2 => "id_mask_jump2.png",
            1 => "id_mask_jump1.png",
            _ => panic!("unexpected jump {jump}"),
         },
         id_mask_fields_rgba(&fields),
      ));
   }

   let golden_dir = golden_dir();
   if std::env::var_os("UPDATE_GOLDENS").as_deref() == Some(std::ffi::OsStr::new("1"))
   {
      fs::create_dir_all(&golden_dir).expect("create reference golden directory");
      for (name, rgba) in &stages
      {
         write_png(&golden_dir.join(name), width, height * 2, rgba);
      }
   }
   for (name, rgba) in stages
   {
      let path = golden_dir.join(name);
      assert!(path.is_file(), "missing committed reference {}", path.display());
      let (golden_width, golden_height, golden_rgba) = read_png(&path);
      assert_eq!((golden_width, golden_height), (width, height * 2));
      assert_eq!(golden_rgba, rgba, "reference mismatch for {name}");
   }
}
