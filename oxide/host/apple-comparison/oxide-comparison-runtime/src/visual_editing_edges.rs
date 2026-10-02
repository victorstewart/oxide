use serde_json::Value;

use oxide_platform_api::{KeyCode, KeyEvent, Modifiers, TextEvent};
use oxide_renderer_api as gfx;
use oxide_renderer_metal as metal;
use oxide_ui_core as ui;
use ui::elements::{TextInput, TextInputState, TextInputStyle};

use super::MtlUploader;

const FIXTURE: &str = include_str!("../../fixtures/visual.json");
const SCALE: f32 = 3.0;

pub(super) struct VisualEditingEdges
{
   fixture: Value,
   font_id: usize,
   stage: Option<usize>,
   long: TextInputState,
   floating: TextInputState,
   composition: TextInputState,
   otp: TextInputState,
   otp_full: TextInputState,
}

impl VisualEditingEdges
{
   pub(super) fn new(font_id: usize) -> Self
   {
      Self {
         fixture: serde_json::from_str(FIXTURE).expect("visual fixture must be valid JSON"),
         font_id,
         stage: None,
         long: TextInputState::new(""),
         floating: TextInputState::new(""),
         composition: TextInputState::new(""),
         otp: TextInputState::new(""),
         otp_full: TextInputState::new(""),
      }
   }

   pub(super) fn draw(&mut self, stage: usize, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = self.fixture["boards"]["visual-editing-edges"].clone();
      for next in advance_stage(&mut self.stage, stage)
      {
         self.apply_stage(next, &spec);
      }
      let input = TextInput {style: text_style(&self.fixture, self.font_id, number(&spec["font_px"])), ..Default::default()};
      let mut uploader = MtlUploader {renderer};
      input.encode(&self.long, rect(&spec["rects"][0]), SCALE, text, &mut uploader, builder);
      input.encode(&self.floating, rect(&spec["rects"][1]), SCALE, text, &mut uploader, builder);
      input.encode(&self.composition, rect(&spec["rects"][2]), SCALE, text, &mut uploader, builder);
      input.encode(&self.otp, rect(&spec["rects"][3]), SCALE, text, &mut uploader, builder);
      input.encode(&self.otp_full, rect(&spec["rects"][4]), SCALE, text, &mut uploader, builder);
   }

   fn apply_stage(&mut self, stage: usize, spec: &Value)
   {
      if stage == 0
      {
         self.long = TextInputState::new("");
         self.long.set_text(spec["long_text"].as_str().unwrap());
         self.long.focus();
         self.long.move_cursor_to_end();
         self.floating = TextInputState::new(spec["floating_placeholder"].as_str().unwrap());
         self.composition = TextInputState::new("");
         self.composition.focus();
         self.composition.handle_text_event(&TextEvent::Commit {text: spec["preedit_base"].as_str().unwrap().into()});
         self.composition.blur();
         self.otp = otp_state(number(&spec["otp_length"]) as usize);
         self.otp_full = otp_state(number(&spec["otp_length"]) as usize);
         self.otp_full.focus();
         self.otp_full.handle_text_event(&TextEvent::Commit {text: spec["otp_full"].as_str().unwrap().into()});
         self.otp_full.blur();
      }
      else if stage == 1
      {
         self.long.blur();
         self.floating.focus();
         for _ in 0..20 {self.floating.tick(100);}
         self.floating.move_cursor_to_end();
         let cursor = self.composition.cursor_index() as u32;
         self.composition.handle_text_event(&TextEvent::Composition {range: cursor..cursor, text: spec["preedit_marked"].as_str().unwrap().into()});
         self.otp = otp_state(number(&spec["otp_length"]) as usize);
         self.otp.focus();
         self.otp.handle_text_event(&TextEvent::Commit {text: spec["otp_partial"].as_str().unwrap().into()});
         self.otp.blur();
      }
      else
      {
         self.floating.blur();
         for _ in 0..20 {self.floating.tick(100);}
         self.composition.focus();
         self.composition.handle_text_event(&TextEvent::Commit {text: spec["preedit_commit"].as_str().unwrap().into()});
         self.composition.blur();
         self.otp.focus();
         self.otp.handle_key(&backspace_event());
         self.otp.handle_key(&backspace_event());
         self.otp.blur();
      }
   }
}

fn otp_state(length: usize) -> TextInputState
{
   let mut state = TextInputState::new("");
   state.configure_one_time_code(length);
   state
}

fn backspace_event() -> KeyEvent
{
   KeyEvent {code: KeyCode::Backspace, chars: None, repeat: false, modifiers: Modifiers::empty()}
}

fn advance_stage(current: &mut Option<usize>, target: usize) -> Vec<usize>
{
   let start = current.map_or(0, |stage| if target < stage {0} else {stage + 1});
   *current = Some(target);
   (start..=target.min(2)).collect()
}

fn text_style(fixture: &Value, font_id: usize, font_px: f32) -> TextInputStyle
{
   let mut selection = color(fixture, "blue");
   selection.a = 0.20;
   TextInputStyle {
      font_id, font_px, text: color(fixture, "text"), placeholder: color(fixture, "text"),
      background: gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0), background_focus: gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0), background_invalid: gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0),
      border: color(fixture, "muted"), border_focus: color(fixture, "blue"), border_invalid: color(fixture, "red"),
      caret: color(fixture, "blue"), selection, composition: color(fixture, "blue"), ..Default::default()
   }
}

fn rect(value: &Value) -> gfx::RectF
{
   gfx::RectF::new(number(&value[0]), number(&value[1]), number(&value[2]), number(&value[3]))
}

fn number(value: &Value) -> f32 {value.as_f64().unwrap() as f32}

fn color(fixture: &Value, name: &str) -> gfx::Color
{
   let value = &fixture["palette"][name];
   gfx::Color::from_srgba(number(&value[0]), number(&value[1]), number(&value[2]), number(&value[3]))
}

#[cfg(test)]
mod tests
{
   use super::*;

   #[test]
   fn settled_events_commit_composition_and_delete_partial_otp()
   {
      let mut board = VisualEditingEdges::new(0);
      let spec = board.fixture["boards"]["visual-editing-edges"].clone();
      for stage in 0..3 {board.apply_stage(stage, &spec);}
      assert_eq!(board.otp.text(), "");
      assert_eq!(board.otp_full.text(), "123456");
      assert_eq!(board.composition.text(), "Compose: café");
      assert_eq!(board.floating.text(), "");
   }
}
