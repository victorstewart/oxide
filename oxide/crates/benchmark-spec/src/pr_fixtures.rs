use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

use crate::scenario::ArtifactIdentity;
use crate::schema::BENCHMARK_SPEC_SCHEMA_VERSION;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartupCard
{
   pub id: String,
   pub data_offset: u32,
   pub data_length: u32,
   pub thumbnail_index: u32,
   pub initially_visible: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartupFixture
{
   pub schema_version: u32,
   pub id: String,
   pub data_size_bytes: u32,
   pub data: String,
   pub card_count: u32,
   pub cards: Vec<StartupCard>,
   pub initial_image_indices: Vec<u32>,
   pub header_id: String,
   pub navigation_id: String,
   pub control_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChatMessage
{
   pub id: String,
   pub sequence: u32,
   pub author_index: u32,
   pub avatar_index: u32,
   pub direction: String,
   pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChatSelectionReplacement
{
   pub message_id: String,
   pub start_utf8: u32,
   pub end_utf8: u32,
   pub replacement: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChatFixture
{
   pub schema_version: u32,
   pub id: String,
   pub message_count: u32,
   pub avatar_count: u32,
   pub messages: Vec<ChatMessage>,
   pub prepend_messages: Vec<ChatMessage>,
   pub append_rate_hz: u32,
   pub typed_text: String,
   pub pasted_text: String,
   pub selection_replacement: ChatSelectionReplacement,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageFileFixture
{
   pub artifact: ArtifactIdentity,
   pub width: u32,
   pub height: u32,
   pub format: String,
   pub color_space: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageDecodeZoomFixture
{
   pub schema_version: u32,
   pub id: String,
   pub source: ImageFileFixture,
   pub thumbnail: ImageFileFixture,
   pub pan_distance_millionths: i32,
   pub pinch_scale_millionths: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardCategories
{
   pub label: u32,
   pub icon_image: u32,
   pub rounded_card: u32,
   pub control: u32,
   pub backdrop_region: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardFixture
{
   pub schema_version: u32,
   pub id: String,
   pub visible_node_count: u32,
   pub categories: DashboardCategories,
   pub clipped_rounded_cards: u32,
   pub shadow_count: u32,
   pub backdrop_blur_count: u32,
   pub leaf_update_sequence: Vec<String>,
   pub update_10_percent_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnduranceFixture
{
   pub schema_version: u32,
   pub id: String,
   pub visible_node_count: u32,
   pub categories: DashboardCategories,
   pub clipped_rounded_cards: u32,
   pub shadow_count: u32,
   pub backdrop_blur_count: u32,
   pub heavy_screen_cycle_count: u32,
   pub tab_switch_count: u32,
   pub animation_frame_count: u32,
   pub tab_count: u32,
   pub initial_tab_index: u32,
   pub heavy_screen_target_id: String,
   pub active_tab_target_id: String,
   pub animation_frame_target_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedRow
{
   pub id: String,
   pub height: u32,
   pub text: String,
   pub thumbnail_index: u32,
   pub favorite: bool,
   pub direction: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedFixture
{
   pub schema_version: u32,
   pub id: String,
   pub row_count: u32,
   pub thumbnail_count: u32,
   pub rows: Vec<FeedRow>,
   pub prepend_rows: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationTransition
{
   pub duration_ms: u32,
   pub curve: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationFixture
{
   pub schema_version: u32,
   pub id: String,
   pub list_item_count: u32,
   pub cycle_count: u32,
   pub transition: NavigationTransition,
   pub interactive_cancel_fraction: f64,
   pub selected_item_id: String,
}

const FEED_TEXTS: &[&str] = &[
   "A measured interface should still feel human.",
   "مرحبا بالعالم — تحديث ثابت",
   "你好，世界 — 固定内容",
   "Frame pacing matters more than a mean.",
   "اختبار التمرير متعدد اللغات",
   "可重复的滚动和图像更新",
   "Pinned assets; identical work; honest pixels.",
   "Status icons: ● ◆ ★ ☺",
];

const CHAT_TEXTS: &[(&str, &str)] = &[
   ("ltr", "Stable frames keep conversation feeling immediate."),
   ("rtl", "مرحبا بالعالم — رسالة ثابتة"),
   ("ltr", "你好，世界 — 固定消息"),
   ("ltr", "Emoji remain grapheme clusters: 👩🏽‍💻 🌍 ✨"),
   ("rtl", "اختبار كتابة واتجاه من اليمين إلى اليسار"),
   ("ltr", "可重复的文本、头像和布局更新"),
   ("ltr", "Measured work stays identical across implementations."),
   ("ltr", "Family status: 👨‍👩‍👧‍👦; flags: 🇺🇳 🇯🇵"),
];

pub(crate) fn validate_startup_fixture(fixture: &StartupFixture) -> Result<()>
{
   ensure!(fixture.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "startup fixture has unsupported schema version {}", fixture.schema_version);
   ensure!(fixture.id == "startup.first-screen", "startup fixture has unexpected id {}", fixture.id);
   ensure!(fixture.data_size_bytes == 24 * 1_024, "startup fixture declares {} data bytes, expected 24576", fixture.data_size_bytes);
   ensure!(fixture.data.as_bytes().len() == fixture.data_size_bytes as usize, "startup fixture materializes {} data bytes but declares {}", fixture.data.len(), fixture.data_size_bytes);
   for (index, byte) in fixture.data.bytes().enumerate()
   {
      ensure!(byte == b'A' + (index % 26) as u8, "startup fixture data byte {} differs from the deterministic sequence", index);
   }
   ensure!(fixture.card_count == 24 && fixture.cards.len() == 24, "startup fixture has {} cards and declares {}, expected 24", fixture.cards.len(), fixture.card_count);
   for (index, card) in fixture.cards.iter().enumerate()
   {
      ensure!(card.id == format!("startup:card:{:02}", index), "startup card {} has unexpected id {}", index, card.id);
      ensure!(card.data_offset == index as u32 * 1_024 && card.data_length == 1_024, "startup card {} has unexpected data span {}+{}", index, card.data_offset, card.data_length);
      ensure!(card.thumbnail_index == index as u32 % 6, "startup card {} has unexpected thumbnail {}", index, card.thumbnail_index);
      ensure!(card.initially_visible == (index < 6), "startup card {} has incorrect initial visibility", index);
   }
   ensure!(fixture.initial_image_indices == [0, 1, 2, 3, 4, 5], "startup fixture initial image indices differ from 0 through 5");
   ensure!(fixture.header_id == "startup:header" && fixture.navigation_id == "startup:navigation" && fixture.control_id == "startup:primary-control", "startup fixture header/navigation/control identities differ from the frozen contract");
   Ok(())
}

pub(crate) fn validate_chat_fixture(fixture: &ChatFixture) -> Result<()>
{
   ensure!(fixture.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "chat fixture has unsupported schema version {}", fixture.schema_version);
   ensure!(fixture.id == "chat.live-update", "chat fixture has unexpected id {}", fixture.id);
   ensure!(fixture.message_count == 5_000 && fixture.messages.len() == 5_000, "chat fixture has {} messages and declares {}, expected 5000", fixture.messages.len(), fixture.message_count);
   ensure!(fixture.avatar_count == 64, "chat fixture declares {} avatars, expected 64", fixture.avatar_count);
   for (index, message) in fixture.messages.iter().enumerate()
   {
      validate_chat_message(message, index as u32, false, fixture.avatar_count)?;
   }
   ensure!(fixture.prepend_messages.len() == 50, "chat fixture has {} prepend messages, expected 50", fixture.prepend_messages.len());
   for (index, message) in fixture.prepend_messages.iter().enumerate()
   {
      validate_chat_message(message, index as u32, true, fixture.avatar_count)?;
   }
   ensure!(fixture.append_rate_hz == 10, "chat fixture append rate is {}Hz, expected 10Hz", fixture.append_rate_hz);
   ensure!(fixture.typed_text == typed_chat_text(), "chat fixture 100-character typing payload differs from the frozen sequence");
   ensure!(fixture.typed_text.chars().count() == 100, "chat fixture typing payload has {} characters, expected 100", fixture.typed_text.chars().count());
   ensure!(fixture.pasted_text == pasted_chat_text(), "chat fixture 10KiB paste payload differs from the frozen sequence");
   ensure!(fixture.pasted_text.as_bytes().len() == 10 * 1_024, "chat fixture paste payload has {} bytes, expected 10240", fixture.pasted_text.len());
   ensure!(fixture.selection_replacement.message_id == "chat:append:16", "chat selection targets unexpected message {}", fixture.selection_replacement.message_id);
   ensure!(fixture.selection_replacement.start_utf8 == 0 && fixture.selection_replacement.end_utf8 == 6 && fixture.selection_replacement.replacement == "Oxide", "chat selection/replacement differs from the frozen contract");
   Ok(())
}

pub(crate) fn validate_image_decode_zoom_fixture(spec_root: &Path, fixture: &ImageDecodeZoomFixture) -> Result<()>
{
   ensure!(fixture.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "image fixture has unsupported schema version {}", fixture.schema_version);
   ensure!(fixture.id == "image.decode-zoom", "image fixture has unexpected id {}", fixture.id);
   validate_png_fixture(spec_root, &fixture.source, "source", "assets/image-decode-zoom-source-v1.png", "ef64a0ed3d87525d904607414ceafe795c6da9071bae94265a25be8feee5ec17", 4_096, 3_072)?;
   validate_png_fixture(spec_root, &fixture.thumbnail, "thumbnail", "assets/image-decode-zoom-thumbnail-v1.png", "05914fd43be375b4612259e955ef1bb06ef303c8b3c32cc27ae7681e99cb1fb9", 384, 288)?;
   ensure!(fixture.pan_distance_millionths == 450_000, "image fixture pan distance is {}, expected 450000", fixture.pan_distance_millionths);
   ensure!(fixture.pinch_scale_millionths == 2_000_000, "image fixture pinch scale is {}, expected 2000000", fixture.pinch_scale_millionths);
   Ok(())
}

fn validate_chat_message(message: &ChatMessage, index: u32, prepend: bool, avatar_count: u32) -> Result<()>
{
   let prefix = if prepend {"chat:prepend"} else {"chat:message"};
   ensure!(message.id == format!("{prefix}:{index:04}"), "chat message {} has unexpected id {}", index, message.id);
   ensure!(message.sequence == if prepend {index} else {index + 50}, "chat message {} has unexpected sequence {}", index, message.sequence);
   ensure!(message.author_index == index % avatar_count && message.avatar_index == index % avatar_count, "chat message {} has unexpected author/avatar", index);
   let (direction, text) = CHAT_TEXTS[index as usize % CHAT_TEXTS.len()];
   ensure!(message.direction == direction && message.text == text, "chat message {} has unexpected direction or text", index);
   Ok(())
}

fn typed_chat_text() -> String
{
   "abcdefghijklmnopqrstuvwxyz".chars().cycle().take(100).collect()
}

fn pasted_chat_text() -> String
{
   "0123456789abcdef".repeat(640)
}

fn validate_png_fixture(spec_root: &Path, image: &ImageFileFixture, label: &str, expected_path: &str, expected_sha256: &str, width: u32, height: u32) -> Result<()>
{
   ensure!(image.artifact.path == expected_path && image.artifact.sha256 == expected_sha256, "image {} artifact identity differs from the frozen deterministic bytes", label);
   ensure!(image.width == width && image.height == height, "image {} dimensions are {}x{}, expected {}x{}", label, image.width, image.height, width, height);
   ensure!(image.format == "png" && image.color_space == "srgb", "image {} format/color space differs from png/srgb", label);
   let bytes = fs::read(spec_root.join(&image.artifact.path))?;
   ensure!(format!("{:x}", Sha256::digest(&bytes)) == image.artifact.sha256, "image {} bytes differ from fixture SHA-256", label);
   ensure!(bytes.len() >= 24 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" && &bytes[12..16] == b"IHDR", "image {} is not a PNG with an IHDR", label);
   let observed_width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
   let observed_height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
   ensure!(observed_width == width && observed_height == height, "image {} PNG dimensions are {}x{}, expected {}x{}", label, observed_width, observed_height, width, height);
   Ok(())
}

pub(crate) fn validate_dashboard_fixture(fixture: &DashboardFixture) -> Result<()>
{
   ensure!(fixture.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "dashboard fixture has unsupported schema version {}", fixture.schema_version);
   ensure!(fixture.id == "dashboard.mixed-static", "dashboard fixture has unexpected id {}", fixture.id);
   ensure!(fixture.visible_node_count == 300, "dashboard fixture has {} visible nodes, expected 300", fixture.visible_node_count);
   ensure!(fixture.categories.label == 176, "dashboard fixture has {} labels, expected 176", fixture.categories.label);
   ensure!(fixture.categories.icon_image == 64, "dashboard fixture has {} icon/images, expected 64", fixture.categories.icon_image);
   ensure!(fixture.categories.rounded_card == 32, "dashboard fixture has {} rounded cards, expected 32", fixture.categories.rounded_card);
   ensure!(fixture.categories.control == 24, "dashboard fixture has {} controls, expected 24", fixture.categories.control);
   ensure!(fixture.categories.backdrop_region == 4, "dashboard fixture has {} backdrop regions, expected 4", fixture.categories.backdrop_region);
   let category_total = fixture.categories.label
      .checked_add(fixture.categories.icon_image)
      .and_then(|total| total.checked_add(fixture.categories.rounded_card))
      .and_then(|total| total.checked_add(fixture.categories.control))
      .and_then(|total| total.checked_add(fixture.categories.backdrop_region));
   ensure!(category_total == Some(fixture.visible_node_count), "dashboard category counts do not total the visible-node count");
   ensure!(fixture.clipped_rounded_cards == 32, "dashboard fixture has {} clipped rounded cards, expected 32", fixture.clipped_rounded_cards);
   ensure!(fixture.shadow_count == 32, "dashboard fixture has {} shadows, expected 32", fixture.shadow_count);
   ensure!(fixture.backdrop_blur_count == 4, "dashboard fixture has {} backdrop blurs, expected 4", fixture.backdrop_blur_count);
   ensure!(fixture.leaf_update_sequence.len() == 20, "dashboard fixture has {} leaf updates, expected 20", fixture.leaf_update_sequence.len());
   for (index, id) in fixture.leaf_update_sequence.iter().enumerate()
   {
      ensure!(id == &format!("dashboard:label:{:03}", index * 7 + 3), "dashboard leaf update {} has unexpected id {}", index, id);
   }
   ensure!(fixture.update_10_percent_ids.len() == 30, "dashboard fixture has {} ten-percent updates, expected 30", fixture.update_10_percent_ids.len());
   for (index, id) in fixture.update_10_percent_ids.iter().enumerate()
   {
      ensure!(id == &format!("dashboard:node:{:03}", index * 9 % 300), "dashboard ten-percent update {} has unexpected id {}", index, id);
   }
   Ok(())
}

pub(crate) fn validate_endurance_fixture(fixture: &EnduranceFixture) -> Result<()>
{
   ensure!(fixture.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "endurance fixture has unsupported schema version {}", fixture.schema_version);
   ensure!(fixture.id == "endurance.churn", "endurance fixture has unexpected id {}", fixture.id);
   ensure!(fixture.visible_node_count == 300, "endurance fixture has {} visible nodes, expected 300", fixture.visible_node_count);
   ensure!(fixture.categories.label == 176, "endurance fixture has {} labels, expected 176", fixture.categories.label);
   ensure!(fixture.categories.icon_image == 64, "endurance fixture has {} icon/images, expected 64", fixture.categories.icon_image);
   ensure!(fixture.categories.rounded_card == 32, "endurance fixture has {} rounded cards, expected 32", fixture.categories.rounded_card);
   ensure!(fixture.categories.control == 24, "endurance fixture has {} controls, expected 24", fixture.categories.control);
   ensure!(fixture.categories.backdrop_region == 4, "endurance fixture has {} backdrop regions, expected 4", fixture.categories.backdrop_region);
   let category_total = fixture.categories.label
      .checked_add(fixture.categories.icon_image)
      .and_then(|total| total.checked_add(fixture.categories.rounded_card))
      .and_then(|total| total.checked_add(fixture.categories.control))
      .and_then(|total| total.checked_add(fixture.categories.backdrop_region));
   ensure!(category_total == Some(fixture.visible_node_count), "endurance category counts do not total the visible-node count");
   ensure!(fixture.clipped_rounded_cards == 32 && fixture.shadow_count == 32 && fixture.backdrop_blur_count == 4, "endurance visual work differs from the frozen dashboard-derived contract");
   ensure!(fixture.heavy_screen_cycle_count == 100, "endurance fixture has {} heavy-screen cycles, expected 100", fixture.heavy_screen_cycle_count);
   ensure!(fixture.tab_switch_count == 500, "endurance fixture has {} tab switches, expected 500", fixture.tab_switch_count);
   ensure!(fixture.animation_frame_count == 600, "endurance fixture has {} animation frames, expected 600", fixture.animation_frame_count);
   ensure!(fixture.tab_count == 2 && fixture.initial_tab_index == 0, "endurance tab contract must begin on tab zero of two");
   ensure!(fixture.heavy_screen_target_id == "endurance:heavy-screen-visible", "endurance heavy-screen target differs from the frozen contract");
   ensure!(fixture.active_tab_target_id == "endurance:active-tab", "endurance active-tab target differs from the frozen contract");
   ensure!(fixture.animation_frame_target_id == "endurance:animation-frame", "endurance animation target differs from the frozen contract");
   Ok(())
}

pub(crate) fn validate_feed_fixture(fixture: &FeedFixture) -> Result<()>
{
   ensure!(fixture.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "feed fixture has unsupported schema version {}", fixture.schema_version);
   ensure!(fixture.id == "feed.variable-scroll", "feed fixture has unexpected id {}", fixture.id);
   ensure!(fixture.row_count == 2_000, "feed fixture declares {} rows, expected 2000", fixture.row_count);
   ensure!(fixture.rows.len() == fixture.row_count as usize, "feed fixture contains {} rows but declares {}", fixture.rows.len(), fixture.row_count);
   ensure!(fixture.thumbnail_count == 128, "feed fixture declares {} thumbnails, expected 128", fixture.thumbnail_count);
   for (index, row) in fixture.rows.iter().enumerate()
   {
      ensure!(row.id == format!("feed:item:{:04}", index), "feed row {} has unexpected id {}", index, row.id);
      ensure!(row.height == 68 + (index as u32 * 17 % 53), "feed row {} has unexpected height {}", index, row.height);
      ensure!(row.text == FEED_TEXTS[index % FEED_TEXTS.len()], "feed row {} has unexpected text", index);
      ensure!(row.thumbnail_index == index as u32 % fixture.thumbnail_count, "feed row {} has unexpected thumbnail {}", index, row.thumbnail_index);
      ensure!(!row.favorite, "feed row {} must begin unfavorited", index);
      let direction = if index % 8 == 1 || index % 8 == 4 {"rtl"} else {"ltr"};
      ensure!(row.direction == direction, "feed row {} has unexpected direction {}", index, row.direction);
   }
   ensure!(fixture.prepend_rows.len() == 20, "feed fixture has {} prepend rows, expected 20", fixture.prepend_rows.len());
   for (index, id) in fixture.prepend_rows.iter().enumerate()
   {
      ensure!(id == &format!("feed:prepend:{:02}", index), "feed prepend row {} has unexpected id {}", index, id);
   }
   Ok(())
}

pub(crate) fn validate_navigation_fixture(fixture: &NavigationFixture) -> Result<()>
{
   ensure!(fixture.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "navigation fixture has unsupported schema version {}", fixture.schema_version);
   ensure!(fixture.id == "navigation.modal", "navigation fixture has unexpected id {}", fixture.id);
   ensure!(fixture.list_item_count == 12, "navigation fixture has {} list items, expected 12", fixture.list_item_count);
   ensure!(fixture.cycle_count == 4, "navigation fixture has {} cycles, expected 4", fixture.cycle_count);
   ensure!(fixture.transition.duration_ms == 300, "navigation fixture transition is {}ms, expected 300ms", fixture.transition.duration_ms);
   ensure!(fixture.transition.curve == "ease-in-out", "navigation fixture has unexpected transition curve {}", fixture.transition.curve);
   ensure!(fixture.interactive_cancel_fraction == 0.5, "navigation fixture cancel fraction is {}, expected 0.5", fixture.interactive_cancel_fraction);
   ensure!(fixture.selected_item_id == "navigation:item:05", "navigation fixture has unexpected selected item {}", fixture.selected_item_id);
   Ok(())
}
