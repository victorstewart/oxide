use std::fs;
use std::path::Path;

use oxide_apple_comparison_controller::{compile_macos_trusted_input_trace, MacOsTrustedInputCommand};
use oxide_benchmark_spec::{TraceEvent, TraceOperation, TraceValue};

#[test]
fn supported_trace_compiles_click_wheel_text_key_drag_and_application_stimuli()
{
   let events = vec![
      pointer(0, TraceOperation::PointerDown, 1, 500_000, 500_000, Some("grid:tile:07500")),
      pointer(50_000, TraceOperation::PointerUp, 1, 500_000, 500_000, Some("grid:tile:07500")),
      target(100_000, TraceOperation::Navigate, "grid:detail:07500"),
      TraceEvent {
         at_us: 200_000,
         op: TraceOperation::Wheel,
         pointer: None,
         x_millionths: None,
         y_millionths: None,
         delta_x_millionths: Some(0),
         delta_y_millionths: Some(125_000),
         target: Some(String::from("grid:collection")),
         value: None,
         state_id: None,
      },
      text(300_000, TraceOperation::CommitText, "chat:composer", "Oxide"),
      text(400_000, TraceOperation::KeyDown, "chat:composer", "return"),
      text(400_001, TraceOperation::KeyUp, "chat:composer", "return"),
      pointer(500_000, TraceOperation::PointerDown, 1, 500_000, 850_000, None),
      pointer(580_000, TraceOperation::PointerMove, 1, 500_000, 550_000, None),
      pointer(660_000, TraceOperation::PointerUp, 1, 500_000, 250_000, None),
      target(700_000, TraceOperation::ResourceArrival, "image:decoded"),
      target(800_000, TraceOperation::Mutate, "dashboard:update-count"),
   ];
   let plan = compile_macos_trusted_input_trace(&events).expect("supported trusted-input plan");
   assert_eq!(plan.commands.len(), 5);
   assert_eq!(plan.application_stimuli.len(), 2);
   assert!(matches!(plan.commands[0], MacOsTrustedInputCommand::Click {first_event_index: 0, last_event_index: 2, ..}));
   assert!(matches!(plan.commands[1], MacOsTrustedInputCommand::Wheel {event_index: 3, ..}));
   assert!(matches!(plan.commands[2], MacOsTrustedInputCommand::Text {event_index: 4, ..}));
   assert!(matches!(plan.commands[3], MacOsTrustedInputCommand::KeyPair {key_down_event_index: 5, key_up_event_index: 6, ..}));
   assert!(matches!(plan.commands[4], MacOsTrustedInputCommand::SinglePointerDrag {first_event_index: 7, last_event_index: 9, duration_us: 160_000, ..}));
   assert_eq!(plan.application_stimuli[0].operation, TraceOperation::ResourceArrival);
   assert_eq!(plan.application_stimuli[1].operation, TraceOperation::Mutate);
}

#[test]
fn pinch_multipointer_fails_with_an_explicit_reason()
{
   let events = vec![
      pointer(0, TraceOperation::PointerDown, 1, 375_000, 500_000, None),
      pointer(0, TraceOperation::PointerDown, 2, 625_000, 500_000, None),
   ];
   let error = compile_macos_trusted_input_trace(&events).expect_err("multipointer must fail");
   assert!(error.to_string().contains("pinch/multipointer"));
}

#[test]
fn pointer_cancel_fails_with_an_explicit_reason()
{
   let events = vec![
      pointer(0, TraceOperation::PointerDown, 1, 500_000, 500_000, None),
      pointer(100_000, TraceOperation::PointerCancel, 1, 500_000, 500_000, None),
   ];
   let error = compile_macos_trusted_input_trace(&events).expect_err("pointer cancel must fail");
   assert!(error.to_string().contains("pointer-cancel"));
}

#[test]
fn ime_composition_fails_with_an_explicit_reason()
{
   let error = compile_macos_trusted_input_trace(&[target(0, TraceOperation::ImeStart, "chat:composer")]).expect_err("IME must fail");
   assert!(error.to_string().contains("IME composition"));
}

#[test]
fn frozen_text_selection_and_replacement_compile_as_one_bounded_command()
{
   let focus = text(0, TraceOperation::Focus, "chat:append:16", "0:6");
   let replacement = text(500_000, TraceOperation::CommitText, "chat:append:16", "Oxide");
   let plan = compile_macos_trusted_input_trace(&[focus, replacement]).expect("frozen select-replace command");
   assert!(matches!(plan.commands.as_slice(), [MacOsTrustedInputCommand::SelectReplace {
      first_event_index: 0,
      last_event_index: 1,
      target,
      selection_start_utf8: 0,
      selection_end_utf8: 6,
      replacement,
   }] if target == "chat:append:16" && replacement == "Oxide"));
}

#[test]
fn arbitrary_text_selection_ranges_fail_closed()
{
   for events in [
      vec![text(0, TraceOperation::Focus, "chat:append:16", "1:6"), text(1, TraceOperation::CommitText, "chat:append:16", "Oxide")],
      vec![text(0, TraceOperation::Focus, "chat:append:16", "0:7"), text(1, TraceOperation::CommitText, "chat:append:16", "Oxide")],
      vec![text(0, TraceOperation::Focus, "chat:message:4096", "0:6"), text(1, TraceOperation::CommitText, "chat:message:4096", "Oxide")],
      vec![text(0, TraceOperation::Focus, "chat:append:16", "0:6"), text(1, TraceOperation::CommitText, "chat:append:16", "Other")],
   ]
   {
      let error = compile_macos_trusted_input_trace(&events).expect_err("unsafe selection must fail");
      assert!(error.to_string().contains("frozen"));
   }
}

#[test]
fn favorite_mutation_is_user_input_while_other_mutations_are_application_stimuli()
{
   let favorite = target(0, TraceOperation::Mutate, "feed:item:0300:favorite");
   let append = target(1, TraceOperation::Mutate, "chat:append");
   let plan = compile_macos_trusted_input_trace(&[favorite, append]).expect("mutation classification");
   assert!(matches!(plan.commands.as_slice(), [MacOsTrustedInputCommand::Click {target, ..}] if target == "feed:item:0300:favorite"));
   assert_eq!(plan.application_stimuli.len(), 1);
   assert_eq!(plan.application_stimuli[0].target.as_deref(), Some("chat:append"));
}

#[test]
fn current_grid_navigation_trace_compiles_to_two_clicks()
{
   let events = current_trace("grid-select-detail-back.json");
   let plan = compile_macos_trusted_input_trace(&events).expect("current grid navigation trace");
   assert!(matches!(plan.commands.as_slice(), [
      MacOsTrustedInputCommand::Click {first_event_index: 0, last_event_index: 2, ..},
      MacOsTrustedInputCommand::Click {first_event_index: 3, last_event_index: 5, ..},
   ]));
}

#[test]
fn current_frozen_override_traces_compile_to_exact_typed_commands()
{
   let image = compile_macos_trusted_input_trace(&current_trace("image-pinch.json")).expect("current image pinch override");
   assert!(matches!(image.commands.as_slice(), [MacOsTrustedInputCommand::ImagePinchOverride {
      first_event_index: 0,
      last_event_index: 17,
      target,
      start_x_millionths: 0,
      start_y_millionths: 500_000,
      end_x_millionths: 1_000_000,
      end_y_millionths: 500_000,
      duration_us: 2_000_000,
   }] if target == "image.zoom"));

   let navigation = compile_macos_trusted_input_trace(&current_trace("navigation-interactive-cancel.json")).expect("current navigation cancellation override");
   assert!(matches!(navigation.commands.as_slice(), [MacOsTrustedInputCommand::NavigationInteractiveCancelOverride {
      first_event_index: 0,
      last_event_index: 4,
      target,
      start_x_millionths: 950_000,
      start_y_millionths: 500_000,
      end_x_millionths: 500_000,
      end_y_millionths: 500_000,
      duration_us: 750_000,
   }] if target == "navigation.table"));
}

#[test]
fn frozen_override_traces_reject_every_timestamp_mutation()
{
   for name in ["image-pinch.json", "navigation-interactive-cancel.json"]
   {
      let events = current_trace(name);
      for index in 0..events.len()
      {
         let mut mutated = events.clone();
         mutated[index].at_us += 1;
         let error = compile_macos_trusted_input_trace(&mutated).expect_err("mutated override trace must fail closed");
         assert!(error.to_string().contains("frozen"), "{name} event {index}: {error:#}");
      }
   }
}

#[test]
fn current_chat_select_replace_trace_compiles_to_the_typed_command()
{
   let plan = compile_macos_trusted_input_trace(&current_trace("chat-select-replace.json")).expect("current chat select-replace trace");
   assert!(matches!(plan.commands.as_slice(), [MacOsTrustedInputCommand::SelectReplace {first_event_index: 0, last_event_index: 1, ..}]));
}

#[test]
fn lifecycle_transitions_fail_until_the_controller_owns_receipted_activation()
{
   for operation in [TraceOperation::Background, TraceOperation::Foreground]
   {
      let error = compile_macos_trusted_input_trace(&[target(0, operation, "application:lifecycle")]).expect_err("lifecycle must fail closed");
      assert!(error.to_string().contains("controller-owned activation or suspension"));
   }
}

fn current_trace(name: &str) -> Vec<TraceEvent>
{
   let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/comparative/specs/v1/traces").join(name);
   let bytes = fs::read(&path).unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
   serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("decoding {}: {error}", path.display()))
}

fn pointer(at_us: u64, op: TraceOperation, pointer: u32, x: i32, y: i32, target: Option<&str>) -> TraceEvent
{
   TraceEvent {
      at_us,
      op,
      pointer: Some(pointer),
      x_millionths: Some(x),
      y_millionths: Some(y),
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: target.map(String::from),
      value: None,
      state_id: None,
   }
}

fn target(at_us: u64, op: TraceOperation, target: &str) -> TraceEvent
{
   TraceEvent {
      at_us,
      op,
      pointer: None,
      x_millionths: None,
      y_millionths: None,
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: Some(String::from(target)),
      value: None,
      state_id: None,
   }
}

fn text(at_us: u64, op: TraceOperation, target_id: &str, value: &str) -> TraceEvent
{
   TraceEvent {
      value: Some(TraceValue::Text(String::from(value))),
      ..target(at_us, op, target_id)
   }
}
