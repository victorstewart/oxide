use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use oxide_web_comparison::{build_route_shipping_manifest, build_shipping_manifest, serve, validate_source_tree, write_manifest};

fn main() -> Result<()>
{
   let mut arguments = std::env::args().skip(1);
   match arguments.next().as_deref()
   {
      Some("validate") =>
      {
         let root = PathBuf::from(arguments.next().context("validate requires SOURCE_ROOT")?);
         validate_source_tree(&root)
      }
      Some("manifest") =>
      {
         let root = PathBuf::from(arguments.next().context("manifest requires SHIPPING_ROOT")?);
         let implementation = arguments.next().context("manifest requires IMPLEMENTATION_ID")?;
         let output = PathBuf::from(arguments.next().context("manifest requires OUTPUT")?);
         let manifest = build_shipping_manifest(&root, &implementation)?;
         write_manifest(&output, &manifest)
      }
      Some("manifest-routes") =>
      {
         let root = PathBuf::from(arguments.next().context("manifest-routes requires SOURCE_ROOT")?);
         let implementation = arguments.next().context("manifest-routes requires IMPLEMENTATION_ID")?;
         let package_root = PathBuf::from(arguments.next().context("manifest-routes requires OXIDE_PACKAGE_ROOT")?);
         let output = PathBuf::from(arguments.next().context("manifest-routes requires OUTPUT")?);
         let manifest = build_route_shipping_manifest(&root, &package_root, &implementation)?;
         write_manifest(&output, &manifest)
      }
      Some("serve") =>
      {
         let root = PathBuf::from(arguments.next().context("serve requires SOURCE_ROOT")?);
         let address = arguments.next().unwrap_or_else(|| String::from("127.0.0.1:4173"));
         serve(&root, &address)
      }
      Some("compare-exact") =>
      {
         let oxide = fs::read(arguments.next().context("compare-exact requires OXIDE_PNG")?).context("reading Oxide exact-static PNG")?;
         let reference = fs::read(arguments.next().context("compare-exact requires REFERENCE_PNG")?).context("reading reference exact-static PNG")?;
         let layout = fs::read(arguments.next().context("compare-exact requires LAYOUT_JSON")?).context("reading exact-static layout")?;
         let scale = arguments.next().context("compare-exact requires CANONICAL_SCALE")?.parse::<u32>().context("parsing exact-static canonical scale")?;
         let output = PathBuf::from(arguments.next().context("compare-exact requires OUTPUT")?);
         let report = oxide_benchmark_spec::compare_exact_static_pngs(&oxide, &reference, &layout, scale)?;
         let web_report = serde_json::json!({
            "schema_version": 1,
            "algorithm": report.algorithm,
            "contender_png_sha256": report.oxide_png_sha256,
            "reference_png_sha256": report.uikit_png_sha256,
            "layout_json_sha256": report.layout_json_sha256,
            "width": report.width,
            "height": report.height,
            "canonical_scale": report.canonical_scale,
            "compared_pixel_count": report.compared_pixel_count,
            "differing_pixel_count": report.differing_pixel_count,
            "differing_channel_count": report.differing_channel_count,
            "maximum_channel_delta": report.maximum_channel_delta,
            "differing_bounds": report.differing_bounds,
            "channel_delta_histogram": report.channel_delta_histogram,
            "accepted": report.accepted,
         });
         let mut bytes = serde_json::to_vec_pretty(&web_report).context("serializing exact-static report")?;
         bytes.push(b'\n');
         let parent = output.parent().context("exact-static report path has no parent")?;
         fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
         let temporary = output.with_extension("tmp");
         fs::write(&temporary, bytes).with_context(|| format!("writing {}", temporary.display()))?;
         fs::rename(&temporary, &output).with_context(|| format!("committing {}", output.display()))?;
         if !report.accepted
         {
            bail!("exact static visual parity rejected: differing_pixels={} differing_channels={} maximum_channel_delta={}", report.differing_pixel_count, report.differing_channel_count, report.maximum_channel_delta);
         }
         Ok(())
      }
      _ => bail!("usage: oxide-web-comparison validate SOURCE_ROOT | manifest SHIPPING_ROOT IMPLEMENTATION_ID OUTPUT | manifest-routes SOURCE_ROOT IMPLEMENTATION_ID OXIDE_PACKAGE_ROOT OUTPUT | serve SOURCE_ROOT [ADDRESS] | compare-exact OXIDE_PNG REFERENCE_PNG LAYOUT_JSON CANONICAL_SCALE OUTPUT"),
   }
}
