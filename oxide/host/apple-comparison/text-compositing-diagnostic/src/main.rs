use std::path::Path;

use oxide_text_compositing_diagnostic as diagnostic;

fn main()
{
   let mut arguments = std::env::args_os().skip(1);
   let Some(font) = arguments.next() else
   {
      eprintln!("usage: oxide-text-compositing-diagnostic <NotoSans-VF.ttf> <output-directory>");
      std::process::exit(2);
   };
   let Some(output) = arguments.next() else
   {
      eprintln!("usage: oxide-text-compositing-diagnostic <NotoSans-VF.ttf> <output-directory>");
      std::process::exit(2);
   };
   if arguments.next().is_some()
   {
      eprintln!("usage: oxide-text-compositing-diagnostic <NotoSans-VF.ttf> <output-directory>");
      std::process::exit(2);
   }
   match diagnostic::run(Path::new(&font), Path::new(&output))
   {
      Ok(summary) => println!("{}", summary.report_path.display()),
      Err(error) =>
      {
         eprintln!("text compositing diagnostic failed: {error}");
         std::process::exit(1);
      }
   }
}
