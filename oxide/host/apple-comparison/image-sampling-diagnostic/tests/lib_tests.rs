use std::{fs, path::PathBuf};

use oxide_image_sampling_diagnostic::{decode_canonical_rgba, sampling_request, sha256, PHASES_MILLIONTHS, ROI_HEIGHT, ROI_WIDTH, SOURCE_HEIGHT, SOURCE_WIDTH};

#[test]
fn canonical_decoder_matches_the_frozen_source_identity()
{
   let source = fs::read(source_path()).expect("frozen source must be readable");
   assert_eq!(sha256(&source), "ef64a0ed3d87525d904607414ceafe795c6da9071bae94265a25be8feee5ec17");
   let (width, height, rgba) = decode_canonical_rgba(&source).expect("frozen source must decode");
   assert_eq!((width, height), (SOURCE_WIDTH, SOURCE_HEIGHT));
   assert_eq!(rgba.len(), SOURCE_WIDTH as usize * SOURCE_HEIGHT as usize * 4);
   assert!(rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
   assert_eq!(sha256(&rgba), "21279282fb9b4954a87b53ac9e5b4b7f1360ed006098e1fbf15b1152469e1215");
}

#[test]
fn sampling_matrix_is_complete_and_bounded()
{
   let request = sampling_request();
   assert_eq!(request.schema_version, 1);
   assert_eq!((request.source_width, request.source_height), (SOURCE_WIDTH, SOURCE_HEIGHT));
   assert_eq!((request.roi_width, request.roi_height), (ROI_WIDTH, ROI_HEIGHT));
   assert_eq!(request.phases_millionths, PHASES_MILLIONTHS);
   assert_eq!(request.cases.len(), 4);
   assert_eq!(request.cases[0].id, "one-to-one");
   assert_eq!(request.cases[1].id, "integer-two-to-one");
   assert_eq!(request.cases[2].destination_width_millionths, 1_170_000_000);
   assert_eq!(request.cases[2].destination_height_millionths, 877_500_000);
   assert_eq!(request.cases[3].destination_width_millionths, 2_340_000_000);
   assert_eq!(request.cases[3].destination_height_millionths, 1_755_000_000);
   assert_eq!(request.cases.len() * request.phases_millionths.len() * 3, 36);
   assert!(u64::from(request.roi_width) * u64::from(request.roi_height) <= 49_152);
}

#[test]
fn canonical_decoder_rejects_invalid_png_bytes()
{
   assert!(decode_canonical_rgba(b"not a PNG").is_err());
}

fn source_path() -> PathBuf
{
   PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/comparative/specs/v1/assets/image-decode-zoom-source-v1.png")
}
