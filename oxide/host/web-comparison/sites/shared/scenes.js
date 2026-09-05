const SCENARIOS = [
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
];
const RAPID_POST_CHECKPOINTS = {
   "startup.first-screen": "fresh-install-ready",
   "dashboard.mixed-static": "leaf-updated",
   "feed.variable-scroll": "favorite-applied",
   "chat.live-update": "append-settled",
   "navigation.modal": "modal-100",
   "image.decode-zoom": "pan-mid",
};

const fixtureUrl = scenarioId => `/specs/fixtures/${scenarioId}.json`;
const atlasUrl = "/specs/assets/neutral-thumbnail-atlas-v1.png";
const thumbnailUrl = "/specs/assets/image-decode-zoom-thumbnail-v1.png";
const imageSourceUrl = "/specs/assets/image-decode-zoom-source-v1.png";
const inlineTextAssets = new Map([
   ["●", [0, 0]], ["◆", [1, 0]], ["★", [2, 0]], ["☺", [3, 0]], ["👩🏽‍💻", [4, 0]],
   ["🌍", [0, 1]], ["✨", [1, 1]], ["👨‍👩‍👧‍👦", [2, 1]], ["🇺🇳", [3, 1]], ["🇯🇵", [4, 1]],
]);
const graphemes = new Intl.Segmenter("und", {granularity: "grapheme"});

const escapeText = value => String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
const inlineText = value => [...graphemes.segment(String(value))].map(({segment}) => {
   const position = inlineTextAssets.get(segment);
   return position ? `<span class="inline-text-asset" role="img" aria-label="${escapeText(segment)}" style="--inline-column:${position[0]};--inline-row:${position[1]}"></span>` : escapeText(segment);
}).join("");
const node = (role, name, count, order, frame, actions = []) => ({role, name, value: String(count), state: ["enabled", "visible"], order, focused: false, actions, frame, count, visible: true});
const roleOrder = {
   "startup.first-screen": ["header", "navigation", "card", "initial-image", "primary-control"],
   "dashboard.mixed-static": ["dashboard", "label", "icon-image", "rounded-card", "control", "backdrop-region"],
   "feed.variable-scroll": ["navigation-bar", "feed", "feed-card", "thumbnail", "favorite-control"],
   "chat.live-update": ["chat-thread", "message", "avatar", "composer", "send-control"],
   "navigation.modal": ["navigation-list", "list-item"],
   "image.decode-zoom": ["image-canvas", "image", "zoom-control"],
};

const canonicalFrame = (scenarioId, role) => {
   if (scenarioId === "startup.first-screen") {
      return {header: [16, 20, 358, 48], navigation: [16, 76, 358, 44], card: [16, 132, 358, 552], "initial-image": [16, 132, 358, 552], "primary-control": [16, 776, 358, 48]}[role];
   }
   if (scenarioId === "dashboard.mixed-static") {
      return role === "dashboard" ? [0, 0, 390, 844] : role === "backdrop-region" ? [12, 42, 366, 586] : [16, 48, 358, 728];
   }
   if (scenarioId === "feed.variable-scroll") {
      return role === "navigation-bar" ? [0, 0, 390, 52] : [0, 52, 390, 792];
   }
   if (scenarioId === "chat.live-update") {
      if (["chat-thread", "message", "avatar"].includes(role)) return [0, 52, 390, 700];
      return role === "composer" ? [12, 780, 318, 48] : [338, 780, 40, 48];
   }
   if (scenarioId === "navigation.modal") {
      return ["modal", "dismiss-control"].includes(role) ? [24, 132, 342, 580] : [0, 0, 390, 844];
   }
   return role === "zoom-control" ? [16, 800, 358, 28] : [0, 52, 390, 740];
};

const canonicalActions = role => {
   if (["primary-control", "control", "favorite-control", "send-control", "list-item", "dismiss-control", "back-control"].includes(role)) return ["activate"];
   if (["feed", "chat-thread"].includes(role)) return ["scroll"];
   if (role === "composer") return ["set-text"];
   if (["image-canvas", "image"].includes(role)) return ["pan", "zoom"];
   if (role === "zoom-control") return ["increment", "decrement"];
   return [];
};

const atlasTile = (className, role, index, label = "") => `<span class="${className} atlas-tile" data-role="${role}" role="img"${label ? ` aria-label="${escapeText(label)}"` : ""} style="--atlas-column:${index % 8};--atlas-row:${Math.floor(index / 8)}"></span>`;
const dashboardCard = index => {
   const labelCount = index < 16 ? 6 : 5;
   const labelBase = index < 16 ? index * 6 : 96 + (index - 16) * 5;
   const labels = Array.from({length: labelCount}, (_, labelIndex) => `<span data-role="label">Node ${labelBase + labelIndex}</span>`).join("");
   const controlIndex = index === 0 ? 3 : labelBase;
   const control = index < 24 ? `<button data-role="control" aria-label="Update Node ${controlIndex}" data-index="${controlIndex}"></button>` : "";
   return `<article class="dashboard-card" data-role="rounded-card" aria-label="Dashboard card ${index + 1}">${atlasTile("dashboard-icon", "icon-image", index * 2, `Icon ${index * 2}`)}${atlasTile("dashboard-icon", "icon-image", index * 2 + 1, `Icon ${index * 2 + 1}`)}<span class="dashboard-labels">${labels}</span>${control}</article>`;
};

const feedRow = (row, top, favoriteId) => `<article class="feed-row" style="top:${top}px;height:${row.height}px"><div class="feed-card" data-role="feed-card">${atlasTile("feed-thumbnail", "thumbnail", row.thumbnail_index, `Thumbnail ${row.thumbnail_index}`)}<div class="feed-copy" dir="${row.direction}">${inlineText(row.text)}</div><small>${escapeText(row.id)}</small><button class="favorite${favoriteId === row.id ? " selected" : ""}" data-role="favorite-control" aria-label="Favorite ${escapeText(row.id)}" data-id="${escapeText(row.id)}"></button></div></article>`;

const settleVisualResources = async root => {
   await document.fonts.ready;
   const images = [...root.querySelectorAll("img")];
   if (root.querySelector(".atlas-tile")) {
      const atlas = new Image();
      atlas.src = atlasUrl;
      images.push(atlas);
   }
   if (root.querySelector(".inline-text-asset")) {
      const inlineAtlas = new Image();
      inlineAtlas.src = "/specs/assets/inline-text-atlas-v1-45px.png";
      images.push(inlineAtlas);
   }
   await Promise.all(images.map(image => image.complete ? image.decode() : new Promise((resolve, reject) => {
      image.addEventListener("load", resolve, {once: true});
      image.addEventListener("error", () => reject(new Error(`image failed to load ${image.currentSrc || image.src}`)), {once: true});
   })));
   await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
};

export class DomSceneAdapter {
   constructor(root, implementationId, serverRendered = false) {
      this.root = root;
      this.implementationId = implementationId;
      this.serverRendered = serverRendered;
      this.scenarioId = "dashboard.mixed-static";
      this.fixture = null;
      this.seed = 0;
      this.generation = 0;
      this.listenerCount = 0;
      this.state = {};
      this.readyPromise = Promise.resolve();
      this.pointer = null;
      this.appendedMessages = [];
   }

   capabilities() {
      return {
         implementation_id: this.implementationId,
         renderer: "dom-css",
         scenarios: SCENARIOS,
         trusted_input_required: true,
         semantic_accessibility: true,
         gpu_pass_timestamps: null,
         gpu_pass_timestamps_reason: "no-equivalent-dom-api",
      };
   }

   async adoptServerRendered(scenarioId, seed = 0) {
      if (!SCENARIOS.includes(scenarioId) || this.root.dataset.scenarioId !== scenarioId) {
         throw new Error(`server-rendered route does not match requested scenario ${scenarioId}`);
      }
      this.scenarioId = scenarioId;
      this.seed = seed;
      this.generation = 1;
      const embeddedFixture = document.getElementById("comparison-fixture");
      if (!embeddedFixture) {
         throw new Error("server-rendered route has no embedded canonical fixture");
      }
      this.fixture = JSON.parse(embeddedFixture.textContent);
      this.initializeState();
      if (scenarioId === "feed.variable-scroll") {
         this.feedOffsets = [0];
         for (const row of this.fixture.rows) {
            this.feedOffsets.push(this.feedOffsets.at(-1) + row.height);
         }
      }
      this.attachListeners();
      this.root.dataset.visualGeneration = String(this.generation);
      this.readyPromise = settleVisualResources(this.root);
      await this.readyPromise;
   }

   async reset(scenarioId, seed, generation) {
      if (!SCENARIOS.includes(scenarioId)) {
         throw new Error(`unsupported scenario ${scenarioId}`);
      }
      this.scenarioId = scenarioId;
      this.seed = Number(seed) || 0;
      this.generation = generation;
      this.readyPromise = this.loadAndRender();
      await this.readyPromise;
   }

   ready() {
      return this.readyPromise;
   }

   async advance(checkpointId) {
      const expected = RAPID_POST_CHECKPOINTS[this.scenarioId];
      if (checkpointId !== expected && !(this.scenarioId === "image.decode-zoom" && checkpointId === "first-visible")) {
         throw new Error(`unsupported rapid checkpoint ${this.scenarioId}/${checkpointId}`);
      }
      if (this.scenarioId === "startup.first-screen") {
         this.state.foreground_count = 1;
         this.state.fresh_install_ready = true;
         this.state.scene_visible = true;
         this.state.lifecycle_state_id = "startup:fresh-install-ready";
      } else if (this.scenarioId === "dashboard.mixed-static") {
         const labels = this.root.querySelectorAll("[data-role=label]");
         for (const index of [3, 10, 17, 24, 31, 38, 45, 52, 59, 66, 73]) {
            labels[index].textContent = `Updated ${++this.state.leaf_update_count}`;
         }
      } else if (this.scenarioId === "feed.variable-scroll") {
         const list = this.root.querySelector(".feed-list");
         const maximum = Math.max(0, this.feedOffsets.at(-1) - 792);
         this.state.scroll_position_millionths = 150000;
         this.state.favorite_id = "feed:item:0300";
         list.scrollTop = maximum * 0.15;
         this.renderFeedWindow(list.scrollTop);
      } else if (this.scenarioId === "chat.live-update") {
         const templates = this.fixture.messages.slice(0, 8);
         this.appendedMessages = Array.from({length: 20}, (_, index) => ({
            ...templates[index % templates.length],
            id: `chat:append:${String(index).padStart(2, "0")}`,
            sequence: 5050 + index,
            author_index: index % 64,
            avatar_index: index % 64,
         }));
         this.state.message_count = this.fixture.messages.length + this.fixture.prepend_messages.length + this.appendedMessages.length;
         this.state.prepend_count = this.fixture.prepend_messages.length;
         this.state.append_count = this.appendedMessages.length;
         this.renderChat();
      } else if (this.scenarioId === "navigation.modal") {
         this.state.route = "detail";
         this.state.modal_visible = true;
         this.renderNavigation();
      } else if (checkpointId === "first-visible") {
         const image = this.root.querySelector(".image-thumbnail");
         this.readyPromise = this.loadImageSource(image);
         await this.readyPromise;
      } else {
         if (this.state.resource_stage !== "visible") {
            throw new Error("image pan requires the first-visible checkpoint");
         }
         this.state.pan_x_millionths = -225000;
         this.state.pan_y_millionths = 0;
         this.state.active_pointer_count = 1;
         this.applyImageTransform();
      }
      this.root.dataset.visualGeneration = String(++this.generation);
   }

   async loadAndRender() {
      const response = await fetch(fixtureUrl(this.scenarioId), {cache: "no-store"});
      if (!response.ok) {
         throw new Error(`fixture request failed ${response.status}`);
      }
      this.fixture = await response.json();
      this.listenerCount = 0;
      this.initializeState();
      this.render();
      await settleVisualResources(this.root);
   }

   initializeState() {
      switch (this.scenarioId) {
      case "startup.first-screen":
         this.state = {foreground_count: 0, background_count: 0, fresh_install_ready: false, scene_visible: false, lifecycle_state_id: null, card_count: this.fixture.cards.length};
         break;
      case "dashboard.mixed-static":
         this.state = {leaf_update_count: 0, bulk_update_count: 0, visible_node_count: this.fixture.visible_node_count};
         break;
      case "feed.variable-scroll":
         this.state = {row_count: this.fixture.rows.length, scroll_position_millionths: 0, favorite_id: null, prepend_count: 0};
         break;
      case "chat.live-update":
         this.state = {message_count: this.fixture.messages.length, prepend_count: 0, append_count: 0, composer_utf8_count: 0, focused_message_id: null, selection_active: false, replacement_applied: false};
         this.appendedMessages = [];
         break;
      case "navigation.modal":
         this.state = {route: "list", modal_visible: false, completed_cycles: 0};
         break;
      case "image.decode-zoom":
         this.state = {resource_stage: "thumbnail", pan_x_millionths: 0, pan_y_millionths: 0, scale_millionths: 1000000, active_pointer_count: 0};
         this.panX = 0;
         this.panY = 0;
         break;
      }
   }

   render() {
      switch (this.scenarioId) {
      case "startup.first-screen": this.renderStartup(); break;
      case "dashboard.mixed-static": this.renderDashboard(); break;
      case "feed.variable-scroll": this.renderFeed(); break;
      case "chat.live-update": this.renderChat(); break;
      case "navigation.modal": this.renderNavigation(); break;
      case "image.decode-zoom": this.renderImage(); break;
      }
      this.root.dataset.visualGeneration = String(this.generation);
   }

   attachListeners() {
      this.listenerCount = 0;
      if (this.scenarioId === "startup.first-screen") {
         const button = this.root.querySelector(".primary-control");
         button?.addEventListener("click", () => {
            this.state.fresh_install_ready = true;
            this.state.lifecycle_state_id = "startup:fresh-install-ready";
            this.root.dataset.visualGeneration = String(++this.generation);
         });
         this.listenerCount += button ? 1 : 0;
      } else if (this.scenarioId === "dashboard.mixed-static") {
         for (const button of this.root.querySelectorAll("button[data-index]")) {
            button.addEventListener("click", () => {
               const index = Number(button.dataset.index);
               const label = this.root.querySelectorAll("[data-role=label]")[index];
               if (label) {
                  label.textContent = `Updated ${++this.state.leaf_update_count}`;
               }
               this.root.dataset.visualGeneration = String(++this.generation);
            });
            this.listenerCount += 1;
         }
      } else if (this.scenarioId === "feed.variable-scroll") {
         const list = this.root.querySelector(".feed-list");
         list?.addEventListener("pointerdown", event => {
            this.pointer = {id: event.pointerId, y: event.clientY};
            list.setPointerCapture(event.pointerId);
         });
         list?.addEventListener("pointermove", event => {
            if (!this.pointer || this.pointer.id !== event.pointerId || event.buttons !== 1) {
               return;
            }
            list.scrollTop -= event.clientY - this.pointer.y;
            this.pointer.y = event.clientY;
         });
         list?.addEventListener("pointerup", event => {
            if (!this.pointer || this.pointer.id !== event.pointerId) {
               return;
            }
            const maximum = Math.max(0, this.feedOffsets.at(-1) - 792);
            const normalized = Math.round((1 - Math.max(0, Math.min(844, event.clientY)) / 844) * 1000000);
            list.scrollTop = normalized / 1000000 * maximum;
            this.renderFeedWindow(list.scrollTop);
            this.state.scroll_position_millionths = normalized;
            this.pointer = null;
            this.root.dataset.visualGeneration = String(++this.generation);
         });
         list?.addEventListener("scroll", () => {
            if (this.feedFramePending) {
               return;
            }
            this.feedFramePending = true;
            requestAnimationFrame(() => {
               this.feedFramePending = false;
               this.renderFeedWindow(list.scrollTop);
               const maximum = Math.max(0, this.feedOffsets.at(-1) - 792);
               this.state.scroll_position_millionths = maximum === 0 ? 0 : Math.round(list.scrollTop / maximum * 1000000);
               this.root.dataset.visualGeneration = String(++this.generation);
            });
         }, {passive: true});
         list?.addEventListener("click", event => {
            const button = event.target.closest(".favorite");
            if (button) {
               button.classList.add("selected");
               this.state.favorite_id = button.dataset.id;
               this.root.dataset.visualGeneration = String(++this.generation);
            }
         });
         this.listenerCount += list ? 5 : 0;
      } else if (this.scenarioId === "chat.live-update") {
         const composer = this.root.querySelector("textarea");
         composer?.addEventListener("input", () => {
            this.state.composer_utf8_count = new TextEncoder().encode(composer.value).length;
            this.root.dataset.visualGeneration = String(++this.generation);
         });
         this.listenerCount += composer ? 1 : 0;
      } else if (this.scenarioId === "navigation.modal") {
         for (const button of this.root.querySelectorAll(".navigation-row")) {
            button.addEventListener("click", () => {
               this.state.route = "detail";
               this.root.innerHTML = `<h1 class="navigation-title">Navigation</h1><section class="navigation-detail" data-role="detail"><h2>Detail</h2><button data-role="back-control">Back</button></section>`;
               this.root.dataset.visualGeneration = String(++this.generation);
            });
            this.listenerCount += 1;
         }
      } else if (this.scenarioId === "image.decode-zoom") {
         const image = this.root.querySelector(".image-thumbnail");
         const stage = this.root.querySelector(".image-stage");
         const slider = this.root.querySelector("input");
         stage?.addEventListener("pointerdown", event => {
            this.pointer = {id: event.pointerId, x: event.clientX, y: event.clientY};
            stage.setPointerCapture(event.pointerId);
            this.state.active_pointer_count = 1;
         });
         stage?.addEventListener("pointermove", event => {
            if (!this.pointer || this.pointer.id !== event.pointerId || event.buttons !== 1) {
               return;
            }
            const maximum = this.fixture.pan_distance_millionths / 1000000;
            this.panX = Math.max(-maximum, Math.min(maximum, this.panX + (event.clientX - this.pointer.x) / 390));
            this.panY = Math.max(-maximum, Math.min(maximum, this.panY + (event.clientY - this.pointer.y) / 844));
            this.state.pan_x_millionths = Math.round(this.panX * 1000000);
            this.state.pan_y_millionths = Math.round(this.panY * 1000000);
            this.pointer = {id: event.pointerId, x: event.clientX, y: event.clientY};
            this.applyImageTransform();
            this.root.dataset.visualGeneration = String(++this.generation);
         });
         stage?.addEventListener("pointerup", event => {
            if (this.pointer?.id === event.pointerId) {
               this.pointer = null;
               this.state.active_pointer_count = 0;
               this.root.dataset.visualGeneration = String(++this.generation);
               this.readyPromise = this.loadImageSource(image);
            }
         });
         slider?.addEventListener("input", () => {
            this.state.scale_millionths = Math.round(Number(slider.value) * 1000000);
            this.applyImageTransform();
            this.root.dataset.visualGeneration = String(++this.generation);
         });
         this.listenerCount += (stage ? 3 : 0) + (slider ? 1 : 0);
      }
   }

   renderStartup() {
      const visible = this.fixture.cards.filter(value => value.initially_visible).slice(0, 6);
      this.root.innerHTML = `<h1 class="scene-title startup-title" data-role="header">Production Comparison</h1><div class="startup-navigation" data-role="navigation">First Screen</div><section class="startup-grid">${visible.map(value => `<article class="startup-card" data-role="card">${atlasTile("startup-thumbnail", "initial-image", value.thumbnail_index, `Thumbnail ${value.thumbnail_index}`)}<strong>${escapeText(value.id)}</strong><p>${escapeText(this.fixture.data.slice(value.data_offset, value.data_offset + 48))}</p></article>`).join("")}</section><button class="primary-control" data-role="primary-control">Continue</button>`;
      this.attachListeners();
   }

   renderDashboard() {
      this.root.innerHTML = `<section class="dashboard-surface" data-role="dashboard" aria-label="Dashboard"><div class="dashboard-backdrops">${Array.from({length: this.fixture.categories.backdrop_region}, () => `<div data-role="backdrop-region"></div>`).join("")}</div><div class="dashboard-grid">${Array.from({length: this.fixture.categories.rounded_card}, (_, index) => dashboardCard(index)).join("")}</div></section>`;
      this.attachListeners();
   }

   renderFeed() {
      const rows = this.fixture.rows;
      this.feedOffsets = [0];
      for (const row of rows) {
         this.feedOffsets.push(this.feedOffsets.at(-1) + row.height);
      }
      this.root.innerHTML = `<h1 class="scene-title feed-title" data-role="navigation-bar">Measured Feed</h1><section class="feed-list" data-role="feed"><div class="feed-spacer" style="height:${this.feedOffsets.at(-1)}px"><div class="feed-window"></div></div></section>`;
      this.renderFeedWindow(0);
      this.attachListeners();
   }

   renderFeedWindow(scrollTop) {
      const windowRoot = this.root.querySelector(".feed-window");
      if (!windowRoot || !this.fixture?.rows) {
         return;
      }
      const offsets = this.feedOffsets;
      let low = 0;
      let high = offsets.length;
      while (low < high) {
         const middle = (low + high) >>> 1;
         if (offsets[middle] <= scrollTop) {
            low = middle + 1;
         } else {
            high = middle;
         }
      }
      const start = Math.max(0, low - 1);
      let end = start;
      const bottom = scrollTop + 792;
      while (end < this.fixture.rows.length && offsets[end] <= bottom) {
         end += 1;
      }
      windowRoot.innerHTML = this.fixture.rows.slice(start, end).map((row, index) => feedRow(row, offsets[start + index], this.state.favorite_id)).join("");
   }

   renderChat() {
      const messages = [...this.fixture.messages, ...this.appendedMessages].slice(-10);
      this.root.innerHTML = `<h1 class="scene-title chat-title">Live Chat</h1><section class="chat-thread" data-role="chat-thread">${messages.map(message => `<article class="chat-row ${message.direction === "rtl" ? "rtl" : ""}" style="height:${[58, 62, 66, 70, 74, 78, 82, 76, 70, 64][message.sequence % 10]}px">${atlasTile("chat-avatar", "avatar", message.avatar_index, `Avatar ${message.avatar_index}`)}<div class="chat-message" data-role="message" dir="${message.direction}">${inlineText(message.text)}</div></article>`).join("")}</section><textarea class="chat-composer" data-role="composer" aria-label="Message"></textarea><button class="chat-send" data-role="send-control">Send</button>`;
      this.attachListeners();
   }

   renderNavigation() {
      if (this.state.route === "list") {
         this.root.innerHTML = `<h1 class="navigation-title">Navigation</h1><nav class="navigation-list" data-role="navigation-list">${Array.from({length: this.fixture.list_item_count}, (_, index) => `<button class="navigation-row" data-role="list-item" data-index="${index}"><strong>Item ${index + 1}</strong><small>Canonical navigation row</small></button>`).join("")}</nav>`;
      } else {
         const modal = this.state.modal_visible ? `<div class="navigation-overlay"><section class="navigation-modal" data-role="modal"><h2>Modal</h2><p>Canonical modal content</p><p>Transition remains apples-to-apples</p><button data-role="dismiss-control">Done</button></section></div>` : "";
         this.root.innerHTML = `<button class="navigation-back" data-role="back-control">‹ Back</button><h1 class="navigation-detail-title">Detail</h1><section class="navigation-detail" data-role="detail"><strong>Selected destination</strong><small>Canonical navigation detail</small></section>${modal}`;
      }
      this.attachListeners();
   }

   renderImage() {
      this.root.innerHTML = `<h1 class="scene-title image-title">Decode &amp; Zoom</h1><section class="image-stage" data-role="image-canvas"><img class="image-thumbnail" data-role="image" src="${thumbnailUrl}" alt="Decoded benchmark fixture"></section><input class="zoom-control" data-role="zoom-control" aria-label="Zoom" type="range" min="1" max="2" step="0.01" value="1">`;
      this.attachListeners();
   }

   async loadImageSource(image) {
      if (this.state.resource_stage !== "thumbnail") {
         return;
      }
      this.state.resource_stage = "bytes-ready";
      const response = await fetch(imageSourceUrl, {cache: "no-store"});
      if (!response.ok) {
         throw new Error(`image source request failed ${response.status}`);
      }
      const blob = await response.blob();
      this.state.resource_stage = "decoded";
      const source = URL.createObjectURL(blob);
      try {
         image.src = source;
         await image.decode();
         this.state.resource_stage = "uploaded";
         this.state.resource_stage = "visible";
         image.classList.add("source-visible");
         this.applyImageTransform();
         this.root.dataset.visualGeneration = String(++this.generation);
      } finally {
         URL.revokeObjectURL(source);
      }
   }

   applyImageTransform() {
      const image = this.root.querySelector(".image-thumbnail");
      if (!image) {
         return;
      }
      const translateX = this.state.pan_x_millionths / 1000000 * 390;
      const translateY = this.state.pan_y_millionths / 1000000 * 740;
      image.style.transform = `translate(${translateX}px, ${translateY}px) scale(${this.state.scale_millionths / 1000000})`;
   }

   snapshot() {
      return {
         scene: {scenario_id: this.scenarioId, fixture_id: this.fixture.id},
         state: this.stateEnvelope("snapshot"),
         target_geometry: {x: 0, y: 0, width: 390, height: 844, scale: devicePixelRatio},
         logical_counters: {dom_nodes: this.root.querySelectorAll("*").length, listeners: this.listenerCount},
         gpu_pass_timestamps: null,
      };
   }

   visibleRoleCounts() {
      const roles = this.scenarioId === "navigation.modal" && this.state.modal_visible
         ? ["detail", "modal", "dismiss-control", "back-control"]
         : this.scenarioId === "navigation.modal" && this.state.route === "detail"
            ? ["detail", "back-control"]
            : roleOrder[this.scenarioId];
      return roles.map(role => ({role, count: this.root.querySelectorAll(`[data-role="${role}"]`).length}));
   }

   stateEnvelope(checkpointId) {
      return {
         schema_version: 2,
         scenario_id: this.scenarioId,
         checkpoint_id: checkpointId,
         model: this.state,
         visible_role_counts: this.visibleRoleCounts(),
      };
   }

   checkpoint(checkpointId) {
      const rawAccessibility = [...this.root.querySelectorAll("[data-role]")].map((element, order) => {
         const bounds = element.getBoundingClientRect();
         return node(element.dataset.role, element.getAttribute("aria-label") || element.textContent.trim().slice(0, 128), 1, order, [bounds.x, bounds.y, bounds.width, bounds.height], element.matches("button,input,textarea") ? ["activate"] : []);
      });
      const nodes = this.visibleRoleCounts().map(({role, count}, order) => node(role, role, count, order, canonicalFrame(this.scenarioId, role), canonicalActions(role)));
      return {
         state: this.stateEnvelope(checkpointId),
         accessibility: {schema_version: 2, scenario_id: this.scenarioId, checkpoint_id: checkpointId, root_frame: [0, 0, 390, 844], raw_tree_source: "runtime-semantic-tree", nodes},
         raw_accessibility: rawAccessibility,
      };
   }

   teardown() {
      this.root.replaceChildren();
      this.fixture = null;
      this.state = {};
      this.listenerCount = 0;
      this.appendedMessages = [];
   }
}
