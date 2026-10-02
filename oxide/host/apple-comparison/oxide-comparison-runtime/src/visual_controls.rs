
use serde_json::Value;

use oxide_platform_api::TextEvent;
use oxide_renderer_api as gfx;
use oxide_renderer_metal as metal;
use oxide_ui_core as ui;
use ui::elements::{Align, Button, ButtonState, ButtonStyle, Label, LabelWrapMode, Overlay, OverlayState,
   OverlayStyle, PickerState, PickerStyle, PopupStyle, PopupWindow, ProgressBar, Slider,
   SliderState, TextInput, TextInputState, TextInputStyle, Toggle, ToggleState};

use super::MtlUploader;
use super::visual_boards::VisualChunk;

const FIXTURE: &str = include_str!("../../fixtures/visual.json");
const SCALE: f32 = 3.0;

pub(super) struct VisualControls
{
   pub(super) chunks: Vec<VisualChunk>,
   fixture: Value,
   font_id: usize,
   bold_id: usize,
   buttons: [ButtonState; 3],
   toggle: ToggleState,
   sliders: [SliderState; 3],
   inputs: [TextInputState; 4],
   picker: PickerState,
   overlay: OverlayState,
   controls_stage: Option<usize>,
   editing_stage: Option<usize>,
   pickers_stage: Option<usize>,
   timed_controls_at: Option<(usize, u64)>,
   timed_pickers_at: Option<(usize, u64)>,
}

impl VisualControls
{
   pub(super) fn new(font_id: usize, bold_id: usize) -> Self
   {
      let fixture = serde_json::from_str(FIXTURE).expect("visual fixture must be valid JSON");
      let columns = board(&fixture, "visual-pickers")["columns"].as_array().unwrap().iter()
         .map(|column| column.as_array().unwrap().iter().map(|item| item.as_str().unwrap().into()).collect())
         .collect();
      Self {
         chunks: Vec::new(), fixture, font_id, bold_id,
         buttons: core::array::from_fn(|_| ButtonState::default()),
         toggle: ToggleState::default(),
         sliders: core::array::from_fn(|_| SliderState::default()),
         inputs: [TextInputState::new(""), TextInputState::new(""), TextInputState::with_secure("", true), TextInputState::new("")],
         picker: PickerState::from_columns(columns),
         overlay: OverlayState::new(),
         controls_stage: None,
         editing_stage: None,
         pickers_stage: None,
         timed_controls_at: None,
         timed_pickers_at: None,
      }
   }

   pub(super) fn draw(&mut self, name: &str, stage: usize, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      self.chunks.clear();
      match name
      {
         "visual-controls" => self.draw_controls(stage, text, renderer, builder),
         "visual-editing" => self.draw_editing(stage, text, renderer, builder),
         "visual-typography" => self.draw_typography(stage, text, renderer, builder),
         "visual-pickers" => self.draw_pickers(stage, text, renderer, builder),
         _ => {}
      }
   }

   pub(super) fn draw_timed(&mut self, name: &str, stage: usize, elapsed_ms: u64, stage_elapsed_ms: u64, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      self.chunks.clear();
      match name
      {
         "visual-controls" => self.draw_controls_timed(stage, elapsed_ms, stage_elapsed_ms, text, renderer, builder),
         "visual-pickers" => self.draw_pickers_timed(stage, stage_elapsed_ms, text, renderer, builder),
         _ => self.draw(name, stage, text, renderer, builder),
      }
   }

   fn draw_controls(&mut self, stage: usize, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = board(&self.fixture, "visual-controls").clone();
      let style = button_style(&self.fixture);
      for next in advance_stage(&mut self.controls_stage, stage) {
         if next == 0
         {
            self.buttons = core::array::from_fn(|_| ButtonState::default());
            self.buttons[1].disabled = true;
            self.toggle = ToggleState::default();
            self.sliders = core::array::from_fn(|_| SliderState::default());
            self.sliders[0].set(0.0, None);
            self.sliders[1].set(0.5, None);
            self.sliders[2].set(1.0, None);
         }
         else if next == 1
         {
            self.buttons[0].on_pointer_down_with_style_at(&style, 0);
            self.buttons[2].on_pointer_down_with_style_at(&style, 0);
            self.toggle.on_tap();
            step_toggle(&mut self.toggle, 100);
            self.sliders[0].set(0.25, None);
            self.sliders[1].set(0.75, None);
         }
         else
         {
            self.buttons[0].on_pointer_up_with_style_at(&style, 100);
            self.buttons[2].on_pointer_cancel_with_style_at(&style, 100);
            step_toggle(&mut self.toggle, 3_000);
            self.sliders[0].set(0.5, None);
            self.sliders[1].set(1.0, None);
         }
      }
      let mut uploader = MtlUploader {renderer};
      let titles = strings(&spec["button_titles"]);
      for index in 0..3
      {
         let button = Button {text: titles[index].clone(), style};
         button.encode_at(rect(&spec["buttons"][index]), SCALE, text, &mut uploader, &self.buttons[index], elapsed(&spec, stage), builder);
      }
      let toggle = Toggle {style: ui::elements::ToggleStyle {track_on: color(&self.fixture, "green"), track_off: color(&self.fixture, "muted"), ..Default::default()}};
      toggle.encode(rect(&spec["toggle_rect"]), &self.toggle, builder);
      for index in 0..3
      {
         Slider {style: ui::elements::SliderStyle {corner: 8.0, track: color(&self.fixture, "muted"), fill: color(&self.fixture, "blue"), ..Default::default()}, step: None}
            .encode(rect(&spec["slider_rects"][index]), &self.sliders[index], builder);
      }
      for (index, value) in spec["progress_values"].as_array().unwrap().iter().enumerate()
      {
         ProgressBar {value: value.as_f64().map(|v| v as f32), track: color(&self.fixture, "muted"), fill: color(&self.fixture, "blue"), corner: 2.0}
            .encode(rect(&spec["progress_rects"][index]), elapsed(&spec, stage) as f32 / 400.0, builder);
      }
   }

   fn draw_controls_timed(&mut self, stage: usize, elapsed_ms: u64, stage_elapsed_ms: u64, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = board(&self.fixture, "visual-controls").clone();
      let style = button_style(&self.fixture);
      for next in advance_stage(&mut self.controls_stage, stage)
      {
         if next == 0
         {
            self.buttons = core::array::from_fn(|_| ButtonState::default());
            self.buttons[1].disabled = true;
            self.toggle = ToggleState::default();
            self.sliders = core::array::from_fn(|_| SliderState::default());
            self.sliders[0].set(0.0, None);
            self.sliders[1].set(0.5, None);
            self.sliders[2].set(1.0, None);
         }
         else if next == 1
         {
            self.buttons[0].on_pointer_down_with_style_at(&style, elapsed_ms);
            self.buttons[2].on_pointer_down_with_style_at(&style, elapsed_ms);
            self.toggle.on_tap();
            self.sliders[0].set(0.25, None);
            self.sliders[1].set(0.75, None);
         }
         else
         {
            self.buttons[0].on_pointer_up_with_style_at(&style, elapsed_ms);
            self.buttons[2].on_pointer_cancel_with_style_at(&style, elapsed_ms);
            self.sliders[0].set(0.5, None);
            self.sliders[1].set(1.0, None);
         }
      }
      let delta_ms = timed_delta(&mut self.timed_controls_at, stage, stage_elapsed_ms);
      if stage != 0 {self.toggle.step(delta_ms);}
      let mut uploader = MtlUploader {renderer};
      let titles = strings(&spec["button_titles"]);
      for index in 0..3
      {
         let button = Button {text: titles[index].clone(), style};
         button.encode_at(rect(&spec["buttons"][index]), SCALE, text, &mut uploader, &self.buttons[index], elapsed_ms, builder);
      }
      let toggle = Toggle {style: ui::elements::ToggleStyle {track_on: color(&self.fixture, "green"), track_off: color(&self.fixture, "muted"), ..Default::default()}};
      toggle.encode(rect(&spec["toggle_rect"]), &self.toggle, builder);
      for index in 0..3
      {
         Slider {style: ui::elements::SliderStyle {corner: 8.0, track: color(&self.fixture, "muted"), fill: color(&self.fixture, "blue"), ..Default::default()}, step: None}
            .encode(rect(&spec["slider_rects"][index]), &self.sliders[index], builder);
      }
      for (index, value) in spec["progress_values"].as_array().unwrap().iter().enumerate()
      {
         ProgressBar {value: value.as_f64().map(|v| v as f32), track: color(&self.fixture, "muted"), fill: color(&self.fixture, "blue"), corner: 2.0}
            .encode(rect(&spec["progress_rects"][index]), elapsed_ms as f32 / 1_000.0, builder);
      }
   }

   fn draw_editing(&mut self, stage: usize, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = board(&self.fixture, "visual-editing").clone();
      for next in advance_stage(&mut self.editing_stage, stage) {
         if next == 0
         {
            let placeholders = strings(&spec["placeholders"]);
            let initial = strings(&spec["initial"]);
            self.inputs = [TextInputState::new(placeholders[0].clone()), TextInputState::new(placeholders[1].clone()), TextInputState::with_secure(placeholders[2].clone(), true), TextInputState::new(placeholders[3].clone())];
            for (input, value) in self.inputs.iter_mut().zip(initial) {input.set_text(value);}
            self.inputs[3].set_validator(|value| value == "valid");
         }
         else if next == 1
         {
            self.inputs[1].focus();
            self.inputs[1].set_selection(number(&spec["selection"][0]) as usize, number(&spec["selection"][1]) as usize);
            self.inputs[1].tick(100);
         }
         else
         {
            self.inputs[0].focus();
            self.inputs[0].handle_text_event(&TextEvent::Commit {text: spec["settled"][0].as_str().unwrap().into()});
            self.inputs[1].handle_text_event(&TextEvent::Commit {text: spec["insert"].as_str().unwrap().into()});
            self.inputs[1].set_selection(6, 6);
            self.inputs[1].handle_text_event(&TextEvent::Commit {text: "!".into()});
            self.inputs[0].blur();
            self.inputs[1].blur();
            self.inputs[3].focus();
            self.inputs[3].set_selection(0, self.inputs[3].text().chars().count());
            self.inputs[3].handle_text_event(&TextEvent::Commit {text: spec["validator_expected"].as_str().unwrap().into()});
            self.inputs[3].blur();
         }
      }
      let mut selection = color(&self.fixture, "blue");
      selection.a = 0.20;
      let style = TextInputStyle {
         font_id: self.font_id, font_px: number(&spec["font_px"]), text: color(&self.fixture, "text"), placeholder: color(&self.fixture, "text"),
         background: gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0), background_focus: gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0), background_invalid: gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0),
         border: color(&self.fixture, "muted"), border_focus: color(&self.fixture, "blue"), border_invalid: color(&self.fixture, "red"),
         caret: color(&self.fixture, "blue"), selection, composition: color(&self.fixture, "blue"), ..Default::default()
      };
      let input = TextInput {style, ..Default::default()};
      let mut uploader = MtlUploader {renderer};
      for index in 0..4 {input.encode(&self.inputs[index], rect(&spec["rects"][index]), SCALE, text, &mut uploader, builder);}
   }

   fn draw_typography(&mut self, stage: usize, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = board(&self.fixture, "visual-typography");
      assert_eq!(spec["wrap_mode"].as_str(), Some("word-or-grapheme"));
      let mut uploader = MtlUploader {renderer};
      for index in 0..6
      {
         let mut bounds = rect(&spec["rects"][index]);
         let align = match (stage, index)
         {
            (2, 3) => Align::Left, (2, 4) => Align::Center, (2, 5) => Align::Right, _ => Align::Left,
         };
         if index == 1 || (index == 3 && stage == 1) {bounds.w = number(&spec["narrow_width"]);}
         let font_id = if index == 1 || index == 2 {self.bold_id} else {self.font_id};
         Label {text: spec["texts"][stage.min(2)][index].as_str().unwrap().into(), color: color(&self.fixture, "text"), align, wrap: true, font_id, font_px: number(&spec["sizes"][index])}
            .encode_with_wrap_mode(bounds, SCALE, LabelWrapMode::WordOrGrapheme, text, &mut uploader, builder);
      }
   }

   fn draw_pickers(&mut self, stage: usize, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = board(&self.fixture, "visual-pickers").clone();
      for next in advance_stage(&mut self.pickers_stage, stage) {
         if next == 0
         {
            let columns = spec["columns"].as_array().unwrap().iter().map(|column| column.as_array().unwrap().iter().map(|item| item.as_str().unwrap().into()).collect()).collect();
            self.picker = PickerState::from_columns(columns);
            self.picker.set_column_selection(0, number(&spec["selections"][0]) as usize);
            self.picker.set_column_selection(1, number(&spec["selections"][1]) as usize);
            self.overlay = OverlayState::new();
         }
         else if next == 1
         {
            self.picker.scroll_column(0, number(&spec["fractional_delta"]));
            self.picker.scroll_column(1, -number(&spec["fractional_delta"]));
            self.overlay.open();
            step_overlay(&mut self.overlay, number(&spec["popup_elapsed_ms"][1]) as u32);
         }
         else
         {
            self.picker.tick(400);
            self.overlay.open();
            step_overlay(&mut self.overlay, 400);
         }
      }
      let mut uploader = MtlUploader {renderer};
      self.picker.encode(&PickerStyle {font_id: self.font_id, font_px: 18.0, baseline_shift: 0.0, highlight: color(&self.fixture, "panel"), text_color: color(&self.fixture, "text"), ..Default::default()}, rect(&spec["picker_rect"]), SCALE, text, &mut uploader, builder);
      let popup_rect = rect(&spec["popup_rect"]);
      let progress = self.overlay.progress();
      Overlay {style: OverlayStyle {tint: gfx::Color::from_srgba(0.0, 0.0, 0.0, 1.0), alpha: 0.08, blur_sigma: 0.0}}.encode(&self.overlay, gfx::RectF::new(0.0, 0.0, 390.0, 844.0), SCALE, builder);
      if progress > 0.01
      {
         let mut local = ui::DrawListBuilder::new();
         let local_rect = gfx::RectF::new(0.0, 0.0, popup_rect.w, popup_rect.h);
         PopupWindow {style: popup_style(color(&self.fixture, "panel"), 12.0 / popup_rect.w)}.encode(local_rect, SCALE, &mut local);
         label(spec["popup_title"].as_str().unwrap(), self.bold_id, 18.0, Align::Center, color(&self.fixture, "text"), gfx::RectF::new(0.0, popup_rect.h * 0.16, popup_rect.w, 30.0), text, &mut uploader, &mut local);
         label(spec["popup_body"].as_str().unwrap(), self.font_id, 14.0, Align::Center, color(&self.fixture, "text"), gfx::RectF::new(20.0, popup_rect.h * 0.46, popup_rect.w - 40.0, 50.0), text, &mut uploader, &mut local);
         let mut chunk = VisualChunk::new(local, 120, stage as u64);
         let scale = 0.92 + progress * 0.08;
         chunk.slots = vec![gfx::RenderPropertySlotId(1201), gfx::RenderPropertySlotId(1202)];
         chunk.properties = vec![
            gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(1201), revision: stage as u64, value: gfx::RenderPropertyValue::Transform([scale, 0.0, 0.0, scale, popup_rect.x + popup_rect.w * (1.0 - scale) * 0.5, popup_rect.y + popup_rect.h * (1.0 - scale) * 0.5])},
            gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(1202), revision: stage as u64, value: gfx::RenderPropertyValue::Opacity(progress)},
         ];
         self.chunks.push(chunk);
         let popover_count = if stage == 1 {2} else {1};
         for anchor in [&spec["left_anchor"], &spec["right_anchor"]].into_iter().take(popover_count)
         {
            let size = size(&spec["popover_size"]);
            let popover_rect = PopupWindow::place_near_anchor(rect(anchor), size, gfx::RectF::new(0.0, 0.0, 390.0, 844.0), number(&spec["viewport_margin"]));
            PopupWindow {style: popup_style(color(&self.fixture, "panel"), 8.0 / popover_rect.w)}.encode(popover_rect, SCALE, builder);
            label(spec["popover_text"].as_str().unwrap(), self.font_id, 14.0, Align::Center, color(&self.fixture, "text"), popover_rect, text, &mut uploader, builder);
         }
      }
   }

   fn draw_pickers_timed(&mut self, stage: usize, stage_elapsed_ms: u64, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = board(&self.fixture, "visual-pickers").clone();
      for next in advance_stage(&mut self.pickers_stage, stage)
      {
         if next == 0
         {
            let columns = spec["columns"].as_array().unwrap().iter().map(|column| column.as_array().unwrap().iter().map(|item| item.as_str().unwrap().into()).collect()).collect();
            self.picker = PickerState::from_columns(columns);
            self.picker.set_column_selection(0, number(&spec["selections"][0]) as usize);
            self.picker.set_column_selection(1, number(&spec["selections"][1]) as usize);
            self.overlay = OverlayState::new();
         }
         else if next == 1
         {
            self.picker.scroll_column(0, number(&spec["fractional_delta"]));
            self.picker.scroll_column(1, -number(&spec["fractional_delta"]));
            self.overlay.open();
         }
         else {self.overlay.open();}
      }
      let delta_ms = timed_delta(&mut self.timed_pickers_at, stage, stage_elapsed_ms);
      self.picker.tick(delta_ms);
      self.overlay.tick(delta_ms);
      self.encode_pickers(&spec, stage, text, renderer, builder);
   }

   fn encode_pickers(&mut self, spec: &Value, stage: usize, text: &mut ui::elements::TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let mut uploader = MtlUploader {renderer};
      self.picker.encode(&PickerStyle {font_id: self.font_id, font_px: 18.0, baseline_shift: 0.0, highlight: color(&self.fixture, "panel"), text_color: color(&self.fixture, "text"), ..Default::default()}, rect(&spec["picker_rect"]), SCALE, text, &mut uploader, builder);
      let popup_rect = rect(&spec["popup_rect"]);
      let progress = self.overlay.progress();
      Overlay {style: OverlayStyle {tint: gfx::Color::from_srgba(0.0, 0.0, 0.0, 1.0), alpha: 0.08, blur_sigma: 0.0}}.encode(&self.overlay, gfx::RectF::new(0.0, 0.0, 390.0, 844.0), SCALE, builder);
      if progress > 0.01
      {
         let mut local = ui::DrawListBuilder::new();
         let local_rect = gfx::RectF::new(0.0, 0.0, popup_rect.w, popup_rect.h);
         PopupWindow {style: popup_style(color(&self.fixture, "panel"), 12.0 / popup_rect.w)}.encode(local_rect, SCALE, &mut local);
         label(spec["popup_title"].as_str().unwrap(), self.bold_id, 18.0, Align::Center, color(&self.fixture, "text"), gfx::RectF::new(0.0, popup_rect.h * 0.16, popup_rect.w, 30.0), text, &mut uploader, &mut local);
         label(spec["popup_body"].as_str().unwrap(), self.font_id, 14.0, Align::Center, color(&self.fixture, "text"), gfx::RectF::new(20.0, popup_rect.h * 0.46, popup_rect.w - 40.0, 50.0), text, &mut uploader, &mut local);
         let mut chunk = VisualChunk::new(local, 120, stage as u64);
         let scale = 0.92 + progress * 0.08;
         chunk.slots = vec![gfx::RenderPropertySlotId(1201), gfx::RenderPropertySlotId(1202)];
         chunk.properties = vec![
            gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(1201), revision: stage as u64, value: gfx::RenderPropertyValue::Transform([scale, 0.0, 0.0, scale, popup_rect.x + popup_rect.w * (1.0 - scale) * 0.5, popup_rect.y + popup_rect.h * (1.0 - scale) * 0.5])},
            gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(1202), revision: stage as u64, value: gfx::RenderPropertyValue::Opacity(progress)},
         ];
         self.chunks.push(chunk);
         let popover_count = if stage == 1 {2} else {1};
         for anchor in [&spec["left_anchor"], &spec["right_anchor"]].into_iter().take(popover_count)
         {
            let size = size(&spec["popover_size"]);
            let popover_rect = PopupWindow::place_near_anchor(rect(anchor), size, gfx::RectF::new(0.0, 0.0, 390.0, 844.0), number(&spec["viewport_margin"]));
            PopupWindow {style: popup_style(color(&self.fixture, "panel"), 8.0 / popover_rect.w)}.encode(popover_rect, SCALE, builder);
            label(spec["popover_text"].as_str().unwrap(), self.font_id, 14.0, Align::Center, color(&self.fixture, "text"), popover_rect, text, &mut uploader, builder);
         }
      }
   }
}

fn board<'a>(fixture: &'a Value, name: &str) -> &'a Value {&fixture["boards"][name]}
fn number(value: &Value) -> f32 {value.as_f64().unwrap() as f32}
fn strings(value: &Value) -> Vec<String> {value.as_array().unwrap().iter().map(|item| item.as_str().unwrap().into()).collect()}
fn rect(value: &Value) -> gfx::RectF {gfx::RectF::new(number(&value[0]), number(&value[1]), number(&value[2]), number(&value[3]))}
fn size(value: &Value) -> [f32; 2] {[number(&value[0]), number(&value[1])]}
fn color(fixture: &Value, name: &str) -> gfx::Color {let value = &fixture["palette"][name]; gfx::Color::from_srgba(number(&value[0]), number(&value[1]), number(&value[2]), number(&value[3]))}
fn elapsed(spec: &Value, stage: usize) -> u64 {number(&spec["stage_elapsed_ms"][stage.min(2)]) as u64}

fn button_style(fixture: &Value) -> ButtonStyle {ButtonStyle {color: color(fixture, "blue"), color_pressed: gfx::Color::from_srgba(0.18, 0.50, 0.95, 1.0), color_disabled: color(fixture, "muted"), press_animation_ms: 200, ..Default::default()}}
fn popup_style(shell_color: gfx::Color, corner_radius_scale: f32) -> PopupStyle {PopupStyle {blur_tint: gfx::Color::from_srgba(0.0, 0.0, 0.0, 0.0), panel_backdrop_alpha: 0.0, panel_backdrop_sigma: 0.0, corner_radius_scale, border_width_points: 0.0, shell_color, inner_fill_color: gfx::Color::from_srgba(0.0, 0.0, 0.0, 0.0)}}

fn label(value: &str, font_id: usize, font_px: f32, align: Align, color: gfx::Color, rect: gfx::RectF, text: &mut ui::elements::TextCtx, uploader: &mut MtlUploader, builder: &mut ui::DrawListBuilder)
{
   Label {text: value.into(), color, align, wrap: true, font_id, font_px}.encode(rect, SCALE, text, uploader, builder);
}

fn advance_stage(current: &mut Option<usize>, stage: usize) -> Vec<usize>
{
   let stage = stage.min(2);
   if current.is_some_and(|prior| prior > stage) {*current = None;}
   let steps = (current.map_or(0, |prior| prior + 1)..=stage).collect();
   *current = Some(stage);
   steps
}

fn timed_delta(last: &mut Option<(usize, u64)>, stage: usize, stage_elapsed_ms: u64) -> u32
{
   let delta = last.filter(|(prior_stage, _)| *prior_stage == stage).map_or(0, |(_, prior)| stage_elapsed_ms.saturating_sub(prior));
   *last = Some((stage, stage_elapsed_ms));
   delta.min(u32::MAX as u64) as u32
}

fn step_toggle(state: &mut ToggleState, elapsed_ms: u32)
{
   for _ in 0..elapsed_ms.div_ceil(16) {state.step(16);}
}

fn step_overlay(state: &mut OverlayState, elapsed_ms: u32)
{
   for _ in 0..elapsed_ms.div_ceil(16) {state.tick(16);}
}
