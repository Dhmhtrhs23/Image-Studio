//! Image generation tab: prompt, references, parameters, provider routing, streaming, results.
use crate::api::{self, Capability, Endpoint, GenEvent, GenRequest, ImageModel, ImageResult, ProviderRouting};
use crate::refs::RefStrip;
use crate::state::AppState;
use gtk4 as gtk;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone)]
pub struct GeneratedItem {
    pub name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub texture: gtk::gdk::Texture,
    pub width: u32,
    pub height: u32,
}

pub struct GenTab {
    pub root: gtk::Box,
    state: Rc<AppState>,
    window: gtk::ApplicationWindow,
    model: RefCell<Option<ImageModel>>,
    choices: RefCell<HashMap<String, Vec<String>>>,
    pub ref_strip: Rc<RefStrip>,
    prompt: gtk::TextView,
    model_label: gtk::Label,
    n_adj: gtk::Adjustment,
    n_spin: gtk::SpinButton,
    resolution_dd: gtk::DropDown,
    aspect_dd: gtk::DropDown,
    size_entry: gtk::Entry,
    quality_dd: gtk::DropDown,
    format_dd: gtk::DropDown,
    background_dd: gtk::DropDown,
    comp_adj: gtk::Adjustment,
    compression_spin: gtk::SpinButton,
    compression_supported: Cell<bool>,
    seed_check: gtk::CheckButton,
    seed_adj: gtk::Adjustment,
    seed_spin: gtk::SpinButton,
    stream_switch: gtk::Switch,
    user_entry: gtk::Entry,
    only_entry: gtk::Entry,
    order_entry: gtk::Entry,
    ignore_entry: gtk::Entry,
    sort_dd: gtk::DropDown,
    fallbacks_dd: gtk::DropDown,
    options_view: gtk::TextView,
    endpoints_label: gtk::Label,
    generate_btn: gtk::Button,
    stop_btn: gtk::Button,
    spinner: gtk::Spinner,
    progress: gtk::ProgressBar,
    status_label: gtk::Label,
    usage_label: gtk::Label,
    request_view: gtk::TextView,
    req_exp: gtk::Expander,
    stream_preview: gtk::Picture,
    stream_box: gtk::Box,
    results: gtk::FlowBox,
    results_store: RefCell<Vec<GeneratedItem>>,
    busy: Rc<Cell<bool>>,
    cancel: Rc<Cell<bool>>,
    gen_id: Rc<Cell<u64>>,
}

fn text_of(tv: &gtk::TextView) -> String {
    let buf = tv.buffer();
    buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string()
}

fn mono_view(height: i32) -> (gtk::TextView, gtk::ScrolledWindow) {
    let tv = gtk::TextView::new();
    tv.set_monospace(true);
    tv.set_wrap_mode(gtk::WrapMode::WordChar);
    tv.set_editable(false);
    tv.set_cursor_visible(false);
    let sc = gtk::ScrolledWindow::new();
    sc.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    sc.set_min_content_height(height);
    sc.set_child(Some(&tv));
    (tv, sc)
}

fn row_label(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.add_css_class("param-label");
    l
}

impl GenTab {
    pub fn new(state: Rc<AppState>, window: gtk::ApplicationWindow, ref_strip: Rc<RefStrip>) -> Rc<GenTab> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        hbox.set_vexpand(true);

        // ------------------------- LEFT: controls -------------------------
        let left_scroll = gtk::ScrolledWindow::new();
        left_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        left_scroll.set_min_content_width(400);
        let left = gtk::Box::new(gtk::Orientation::Vertical, 10);
        left.set_margin_top(10);
        left.set_margin_bottom(10);
        left.set_margin_start(10);
        left.set_margin_end(10);

        let prompt_title = gtk::Label::new(Some("Prompt"));
        prompt_title.set_xalign(0.0);
        prompt_title.add_css_class("section-title");
        left.append(&prompt_title);
        let prompt = gtk::TextView::new();
        prompt.set_wrap_mode(gtk::WrapMode::WordChar);
        prompt.set_accepts_tab(false);
        let prompt_scroll = gtk::ScrolledWindow::new();
        prompt_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        prompt_scroll.set_min_content_height(120);
        prompt_scroll.add_css_class("card");
        prompt_scroll.set_child(Some(&prompt));
        left.append(&prompt_scroll);

        left.append(&ref_strip.root);

        // Parameters
        let params_frame = gtk::Frame::new(Some("Generation parameters"));
        let grid = gtk::Grid::new();
        grid.set_row_spacing(8);
        grid.set_column_spacing(12);
        grid.set_margin_top(8);
        grid.set_margin_bottom(8);
        grid.set_margin_start(8);
        grid.set_margin_end(8);

        let n_adj = gtk::Adjustment::new(1.0, 1.0, 1.0, 1.0, 1.0, 0.0);
        let n_spin = gtk::SpinButton::new(Some(&n_adj), 1.0, 0);
        n_spin.set_tooltip_text(Some("Number of images (n). Disabled when the model does not support n > 1."));

        let resolution_dd = gtk::DropDown::from_strings(&["(model default)"]);
        let aspect_dd = gtk::DropDown::from_strings(&["(model default)"]);
        let size_entry = gtk::Entry::new();
        size_entry.set_placeholder_text(Some("e.g. 2K or 1024x1024"));
        let quality_dd = gtk::DropDown::from_strings(&["(model default)"]);
        let format_dd = gtk::DropDown::from_strings(&["(model default)"]);
        let background_dd = gtk::DropDown::from_strings(&["(model default)"]);
        let comp_adj = gtk::Adjustment::new(90.0, 0.0, 100.0, 1.0, 10.0, 0.0);
        let compression_spin = gtk::SpinButton::new(Some(&comp_adj), 1.0, 0);
        let seed_check = gtk::CheckButton::with_label("use seed");
        let seed_adj = gtk::Adjustment::new(0.0, 0.0, 2147483647.0, 1.0, 100.0, 0.0);
        let seed_spin = gtk::SpinButton::new(Some(&seed_adj), 1.0, 0);
        let seed_random = gtk::Button::with_label("Random");
        seed_random.add_css_class("flat");
        let stream_switch = gtk::Switch::new();
        stream_switch.set_tooltip_text(Some("Stream partial images as they are generated (SSE)"));
        let stream_switch_cell = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let stream_label = gtk::Label::new(Some("Stream"));
        stream_label.add_css_class("dim-label");
        stream_switch_cell.append(&stream_label);
        stream_switch_cell.append(&stream_switch);
        let user_entry = gtk::Entry::new();
        user_entry.set_placeholder_text(Some("optional stable end-user id"));

        let seed_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        seed_row.append(&seed_check);
        seed_row.append(&seed_spin);
        seed_row.append(&seed_random);

        let rows: Vec<(&str, gtk::Widget)> = vec![
            ("Images (n)", n_spin.clone().upcast()),
            ("Resolution", resolution_dd.clone().upcast()),
            ("Aspect ratio", aspect_dd.clone().upcast()),
            ("Size", size_entry.clone().upcast()),
            ("Quality", quality_dd.clone().upcast()),
            ("Output format", format_dd.clone().upcast()),
            ("Background", background_dd.clone().upcast()),
            ("Compression", compression_spin.clone().upcast()),
            ("Seed", seed_row.clone().upcast()),
            ("User", user_entry.clone().upcast()),
        ];
        for (i, (name, w)) in rows.iter().enumerate() {
            grid.attach(&row_label(name), 0, i as i32, 1, 1);
            grid.attach(w, 1, i as i32, 1, 1);
        }
        params_frame.set_child(Some(&grid));
        left.append(&params_frame);

        // Provider routing
        let prov = gtk::Expander::new(Some("Provider routing"));
        prov.set_expanded(false);
        let pg = gtk::Grid::new();
        pg.set_row_spacing(8);
        pg.set_column_spacing(12);
        pg.set_margin_top(8);
        pg.set_margin_bottom(8);
        pg.set_margin_start(8);
        pg.set_margin_end(8);
        let only_entry = gtk::Entry::new();
        only_entry.set_placeholder_text(Some("slug-a, slug-b"));
        let order_entry = gtk::Entry::new();
        order_entry.set_placeholder_text(Some("slug-a, slug-b"));
        let ignore_entry = gtk::Entry::new();
        ignore_entry.set_placeholder_text(Some("slug-a, slug-b"));
        let sort_dd = gtk::DropDown::from_strings(&["(default)", "price", "throughput", "latency"]);
        let fallbacks_dd = gtk::DropDown::from_strings(&["(default)", "allow fallbacks", "disallow fallbacks"]);
        let options_view = gtk::TextView::new();
        options_view.set_monospace(true);
        options_view.set_wrap_mode(gtk::WrapMode::WordChar);
        let options_scroll = gtk::ScrolledWindow::new();
        options_scroll.set_min_content_height(70);
        options_scroll.set_child(Some(&options_view));
        let prov_rows: Vec<(&str, gtk::Widget)> = vec![
            ("Only", only_entry.clone().upcast()),
            ("Order", order_entry.clone().upcast()),
            ("Ignore", ignore_entry.clone().upcast()),
            ("Sort", sort_dd.clone().upcast()),
            ("Fallbacks", fallbacks_dd.clone().upcast()),
            ("Options JSON", options_scroll.clone().upcast()),
        ];
        for (i, (name, w)) in prov_rows.iter().enumerate() {
            pg.attach(&row_label(name), 0, i as i32, 1, 1);
            pg.attach(w, 1, i as i32, 1, 1);
        }
        let endpoints_label = gtk::Label::new(Some("Select a model to load endpoint capabilities."));
        endpoints_label.set_xalign(0.0);
        endpoints_label.set_wrap(true);
        endpoints_label.add_css_class("dim-label");
        pg.attach(&endpoints_label, 0, prov_rows.len() as i32, 2, 1);
        prov.set_child(Some(&pg));
        left.append(&prov);

        // Request preview
        let req_exp = gtk::Expander::new(Some("Request JSON preview / log"));
        req_exp.set_expanded(false);
        let (request_view, request_scroll) = mono_view(180);
        req_exp.set_child(Some(&request_scroll));
        left.append(&req_exp);

        left_scroll.set_size_request(400, -1);
        left_scroll.set_hexpand(false);
        left_scroll.set_child(Some(&left));
        hbox.append(&left_scroll);
        hbox.append(&gtk::Separator::new(gtk::Orientation::Vertical));

        // ------------------------- RIGHT: results -------------------------
        let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
        right.set_margin_top(10);
        right.set_margin_bottom(10);
        right.set_margin_start(10);
        right.set_margin_end(10);

        let model_label = gtk::Label::new(Some("No model selected"));
        model_label.set_xalign(0.0);
        model_label.set_wrap(true);
        model_label.add_css_class("model-headline");
        right.append(&model_label);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let generate_btn = gtk::Button::with_label("Generate");
        generate_btn.add_css_class("suggested-action");
        generate_btn.set_tooltip_text(Some("Send the request to OpenRouter (Ctrl+Enter)"));
        let stop_btn = gtk::Button::with_label("Stop waiting");
        stop_btn.set_sensitive(false);
        let preview_btn = gtk::Button::from_icon_name("document-properties-symbolic");
        preview_btn.set_tooltip_text(Some("Preview request JSON"));
        let clear_btn = gtk::Button::from_icon_name("edit-clear-all-symbolic");
        clear_btn.set_tooltip_text(Some("Clear results"));
        let save_all_btn = gtk::Button::from_icon_name("document-save-symbolic");
        save_all_btn.set_tooltip_text(Some("Save all results to a folder"));
        let spinner = gtk::Spinner::new();
        actions.append(&generate_btn);
        actions.append(&stop_btn);
        actions.append(&spinner);
        let stream_sep = gtk::Separator::new(gtk::Orientation::Vertical);
        actions.append(&stream_sep);
        actions.append(&stream_switch_cell);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        actions.append(&spacer);
        right.append(&actions);

        let actions2 = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions2.set_halign(gtk::Align::End);
        actions2.append(&preview_btn);
        actions2.append(&clear_btn);
        actions2.append(&save_all_btn);
        right.append(&actions2);

        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        progress.set_visible(false);
        right.append(&progress);

        let status_label = gtk::Label::new(Some("Ready."));
        status_label.set_xalign(0.0);
        status_label.set_wrap(true);
        status_label.add_css_class("status-label");
        right.append(&status_label);

        let usage_label = gtk::Label::new(None);
        usage_label.set_xalign(0.0);
        usage_label.add_css_class("dim-label");
        right.append(&usage_label);

        // Streaming preview
        let stream_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        stream_box.set_visible(false);
        let stream_title = gtk::Label::new(Some("Streaming preview"));
        stream_title.set_xalign(0.0);
        stream_title.add_css_class("section-title");
        stream_box.append(&stream_title);
        let stream_preview = gtk::Picture::new();
        stream_preview.set_content_fit(gtk::ContentFit::Contain);
        stream_preview.set_can_shrink(true);
        stream_preview.set_size_request(-1, 260);
        stream_box.append(&stream_preview);
        right.append(&stream_box);

        let results_scroll = gtk::ScrolledWindow::new();
        results_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        results_scroll.set_vexpand(true);
        let results = gtk::FlowBox::new();
        results.set_selection_mode(gtk::SelectionMode::None);
        results.set_max_children_per_line(2);
        results.set_min_children_per_line(1);
        results.set_column_spacing(10);
        results.set_row_spacing(10);
        results.set_halign(gtk::Align::Start);
        results.set_valign(gtk::Align::Start);
        results.set_margin_top(6);
        results.set_margin_bottom(6);
        results_scroll.set_propagate_natural_width(false);
        results_scroll.set_child(Some(&results));
        right.append(&results_scroll);

        right.set_hexpand(true);
        hbox.append(&right);
        root.append(&hbox);

        let tab = Rc::new(GenTab {
            root,
            state,
            window,
            model: RefCell::new(None),
            choices: RefCell::new(HashMap::new()),
            ref_strip,
            prompt,
            model_label,
            n_adj,
            n_spin,
            resolution_dd,
            aspect_dd,
            size_entry,
            quality_dd,
            format_dd,
            background_dd,
            comp_adj,
            compression_spin,
            compression_supported: Cell::new(false),
            seed_check: seed_check.clone(),
            seed_adj,
            seed_spin: seed_spin.clone(),
            stream_switch: stream_switch.clone(),
            user_entry,
            only_entry,
            order_entry,
            ignore_entry,
            sort_dd,
            fallbacks_dd,
            options_view,
            endpoints_label: endpoints_label.clone(),
            generate_btn: generate_btn.clone(),
            stop_btn: stop_btn.clone(),
            spinner: spinner.clone(),
            progress: progress.clone(),
            status_label: status_label.clone(),
            usage_label,
            request_view,
            req_exp: req_exp.clone(),
            stream_preview: stream_preview.clone(),
            stream_box: stream_box.clone(),
            results: results.clone(),
            results_store: RefCell::new(Vec::new()),
            busy: Rc::new(Cell::new(false)),
            cancel: Rc::new(Cell::new(false)),
            gen_id: Rc::new(Cell::new(0)),
        });

        {
            let t = tab.clone();
            seed_check.connect_toggled(move |c| {
                t.seed_spin.set_sensitive(c.is_active());
            });
        }
        {
            let t = tab.clone();
            seed_random.connect_clicked(move |_| {
                t.seed_spin.set_value(pseudo_random_seed() as f64);
            });
        }
        {
            let t = tab.clone();
            generate_btn.connect_clicked(move |_| t.start_generation());
        }
        {
            let t = tab.clone();
            stop_btn.connect_clicked(move |_| t.cancel_generation());
        }
        {
            let t = tab.clone();
            clear_btn.connect_clicked(move |_| t.clear_results());
        }
        {
            let t = tab.clone();
            save_all_btn.connect_clicked(move |_| t.save_all());
        }
        {
            let t = tab.clone();
            preview_btn.connect_clicked(move |_| t.preview_json());
        }
        {
            let t = tab.clone();
            tab.format_dd.connect_selected_notify(move |_| t.update_format_dependent());
        }
        {
            let t = tab.clone();
            req_exp.connect_expanded_notify(move |e| {
                if e.is_expanded() {
                    t.preview_json();
                }
            });
        }
        // Ctrl+Enter in the prompt triggers generation.
        let key = gtk::EventControllerKey::new();
        {
            let t = tab.clone();
            key.connect_key_pressed(move |_, keyval, _, state| {
                let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
                if ctrl && (keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter) {
                    t.start_generation();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        }
        tab.prompt.add_controller(key);

        tab.apply_model(None);
        {
            let t = tab.clone();
            tab.ref_strip.set_on_message(Box::new(move |m| t.set_status(&m, false)));
        }
        tab
    }

    pub fn set_model(self: &Rc<Self>, model: Option<&ImageModel>) {
        self.apply_model(model);
        if let Some(m) = model {
            self.state.config.borrow_mut().last_model = m.id.clone();
            self.state.save();
        }
    }

    pub fn set_endpoints(&self, eps: &[Endpoint]) {
        if eps.is_empty() {
            self.endpoints_label
                .set_text("Endpoint detail unavailable for this model (it may be routed dynamically).");
            return;
        }
        let mut lines: Vec<String> = Vec::new();
        for e in eps {
            let slug = e.pin_slug().unwrap_or_else(|| "?".into());
            let price: Vec<String> = e.pricing.iter().map(|p| p.label()).collect();
            let passthrough = if e.allowed_passthrough_parameters.is_empty() {
                String::new()
            } else {
                format!(" | passthrough: {}", e.allowed_passthrough_parameters.join(", "))
            };
            lines.push(format!(
                "{} [{}] stream={} | {}{}",
                e.label(),
                slug,
                e.supports_streaming,
                if price.is_empty() { "no pricing listed".into() } else { price.join("; ") },
                passthrough
            ));
        }
        self.endpoints_label.set_text(&lines.join("\n"));
    }

    pub fn focus_prompt(&self) {
        self.prompt.grab_focus();
    }

    fn apply_model(&self, model: Option<&ImageModel>) {
        let m = match model {
            Some(m) => m.clone(),
            None => {
                *self.model.borrow_mut() = None;
                self.model_label.set_text("No model selected");
                self.ref_strip.set_max(0);
                return;
            }
        };
        let caps = m.capabilities();
        self.model_label.set_text(&format!(
            "{}   ({}  ->  {})",
            m.id,
            m.input_modalities(),
            m.output_modalities()
        ));
        *self.model.borrow_mut() = Some(m.clone());

        if let Some(c) = caps.get("n") {
            let min = c.min.unwrap_or(1).max(1);
            let max = c.max.unwrap_or(min).max(min);
            self.n_adj.set_lower(min as f64);
            self.n_adj.set_upper(max as f64);
            self.n_adj.set_value(1.0);
            self.n_spin.set_sensitive(max > 1);
        } else {
            self.n_adj.set_lower(1.0);
            self.n_adj.set_upper(1.0);
            self.n_adj.set_value(1.0);
            self.n_spin.set_sensitive(false);
        }

        self.fill_dd(&self.resolution_dd, "resolution", caps.get("resolution"));
        self.fill_dd(&self.aspect_dd, "aspect_ratio", caps.get("aspect_ratio"));
        self.fill_dd(&self.quality_dd, "quality", caps.get("quality"));
        self.fill_dd(&self.format_dd, "output_format", caps.get("output_format"));
        self.fill_dd(&self.background_dd, "background", caps.get("background"));

        let has_size = caps.contains_key("size");
        self.size_entry.set_sensitive(has_size);
        if !has_size {
            self.size_entry.set_text("");
        }

        self.compression_supported.set(caps.contains_key("output_compression"));
        if let Some(c) = caps.get("output_compression") {
            let min = c.min.unwrap_or(0);
            let max = c.max.unwrap_or(100);
            self.comp_adj.set_lower(min as f64);
            self.comp_adj.set_upper(max as f64);
        }
        self.update_format_dependent();

        let has_seed = caps.contains_key("seed");
        self.seed_check.set_sensitive(has_seed);
        if !has_seed {
            self.seed_check.set_active(false);
        }
        self.seed_spin.set_sensitive(has_seed && self.seed_check.is_active());

        self.stream_switch.set_sensitive(m.supports_streaming);
        if !m.supports_streaming {
            self.stream_switch.set_active(false);
        }

        let max_refs = m.max_refs();
        let has_ref_cap = caps.contains_key("input_references");
        let accepts_image = m.architecture.input_modalities.iter().any(|x| x == "image");
        let effective = if has_ref_cap {
            if max_refs <= 0 { -1 } else { max_refs }
        } else if accepts_image {
            -1
        } else {
            0
        };
        self.ref_strip.set_max(effective);
    }

    fn fill_dd(&self, dd: &gtk::DropDown, key: &str, cap: Option<&Capability>) {
        let mut strings: Vec<String> = Vec::new();
        let mut values: Vec<String> = Vec::new();
        if let Some(c) = cap {
            if c.kind == "enum" {
                if let Some(vs) = &c.values {
                    values = vs.clone();
                }
            }
        }
        if values.is_empty() {
            strings.push("(not supported)".into());
        } else {
            strings.push("(model default)".into());
            for v in &values {
                strings.push(v.clone());
            }
        }
        let refs: Vec<&str> = strings.iter().map(|s| s.as_str()).collect();
        let list = gtk::StringList::new(&refs);
        dd.set_model(Some(&list));
        dd.set_selected(0);
        dd.set_sensitive(!values.is_empty());
        self.choices.borrow_mut().insert(key.to_string(), values);
    }

    fn update_format_dependent(&self) {
        let fmt = self.selected("output_format", &self.format_dd);
        let ok = self.compression_supported.get()
            && matches!(fmt.as_deref(), Some("jpeg") | Some("webp"));
        self.compression_spin.set_sensitive(ok);
    }

    fn selected(&self, key: &str, dd: &gtk::DropDown) -> Option<String> {
        let idx = dd.selected() as usize;
        if idx == 0 {
            return None;
        }
        self.choices
            .borrow()
            .get(key)
            .and_then(|v| v.get(idx - 1).cloned())
    }

    fn build_request(&self, model: &ImageModel, prompt: &str) -> Result<GenRequest, String> {
        let mut r = GenRequest::default();
        r.model = model.id.clone();
        r.prompt = prompt.to_string();

        if self.n_spin.is_sensitive() {
            let n = self.n_spin.value_as_int();
            if n >= 1 {
                r.n = Some(n as u32);
            }
        }
        r.resolution = self.selected("resolution", &self.resolution_dd);
        r.aspect_ratio = self.selected("aspect_ratio", &self.aspect_dd);
        let size = self.size_entry.text().to_string();
        if self.size_entry.is_sensitive() && !size.trim().is_empty() {
            r.size = Some(size.trim().to_string());
        }
        r.quality = self.selected("quality", &self.quality_dd);
        r.output_format = self.selected("output_format", &self.format_dd);
        r.background = self.selected("background", &self.background_dd);
        if self.compression_spin.is_sensitive() {
            let fmt = r.output_format.clone();
            if matches!(fmt.as_deref(), Some("jpeg") | Some("webp")) {
                r.output_compression = Some(self.compression_spin.value_as_int().max(0) as u32);
            }
        }
        if self.seed_check.is_active() && self.seed_spin.is_sensitive() {
            r.seed = Some(self.seed_spin.value_as_int() as i64);
        }
        let refs = self.ref_strip.input_refs();
        if !refs.is_empty() {
            r.input_references = Some(refs);
        }
        let user = self.user_entry.text().to_string();
        if !user.trim().is_empty() {
            r.user = Some(user.trim().to_string());
        } else {
            r.user = self.state.user();
        }

        let mut p = ProviderRouting::default();
        p.only = crate::util::split_csv(&self.only_entry.text());
        p.order = crate::util::split_csv(&self.order_entry.text());
        p.ignore = crate::util::split_csv(&self.ignore_entry.text());
        p.sort = match self.sort_dd.selected() {
            1 => Some(json!("price")),
            2 => Some(json!("throughput")),
            3 => Some(json!("latency")),
            _ => None,
        };
        p.allow_fallbacks = match self.fallbacks_dd.selected() {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        };
        let opts = text_of(&self.options_view);
        if !opts.trim().is_empty() {
            let v: Value = serde_json::from_str(&opts)
                .map_err(|e| format!("Provider options JSON is invalid: {e}"))?;
            if !v.is_null() {
                p.options = Some(v);
            }
        }
        if !p.is_empty() {
            r.provider = Some(p);
        }
        Ok(r)
    }

    pub fn start_generation(self: &Rc<Self>) {
        if self.busy.get() {
            return;
        }
        if !self.state.has_key() {
            self.set_status("No API key. Open the Key dialog from the header bar.", true);
            return;
        }
        let model = match self.model.borrow().clone() {
            Some(m) => m,
            None => {
                self.set_status("Select an image model in the sidebar first.", true);
                return;
            }
        };
        let prompt = text_of(&self.prompt);
        if prompt.trim().is_empty() {
            self.set_status("Enter a prompt.", true);
            return;
        }
        let req = match self.build_request(&model, &prompt) {
            Ok(r) => r,
            Err(e) => {
                self.set_status(&e, true);
                return;
            }
        };
        let stream = self.stream_switch.is_active() && model.supports_streaming;
        let json = serde_json::to_string_pretty(&req.to_json()).unwrap_or_default();
        self.request_view.buffer().set_text(&json);

        let (tx, rx) = async_channel::unbounded::<GenEvent>();
        let client = self.state.client();
        let base = self.state.base_url();
        let key = self.state.api_key();
        let req2 = req.clone();
        std::thread::spawn(move || {
            let mut emit = |ev: GenEvent| {
                let _ = tx.send_blocking(ev);
            };
            api::generate(&client, &base, &key, &req2, stream, &mut emit);
        });

        let gen = self.gen_id.get() + 1;
        self.gen_id.set(gen);
        self.cancel.set(false);
        self.begin_busy(if stream { "Streaming generation..." } else { "Generating..." });

        let tab = self.clone();
        glib::spawn_future_local(async move {
            while let Ok(ev) = rx.recv().await {
                if tab.gen_id.get() != gen {
                    continue;
                }
                let finished = matches!(ev, GenEvent::Done(_) | GenEvent::Error(_));
                tab.handle_event(ev);
                if finished {
                    break;
                }
            }
        });
    }

    fn begin_busy(&self, msg: &str) {
        self.busy.set(true);
        self.generate_btn.set_sensitive(false);
        self.stop_btn.set_sensitive(true);
        self.spinner.start();
        self.progress.set_visible(true);
        self.progress.set_fraction(0.0);
        self.progress.set_text(Some("working..."));
        self.set_status(msg, false);
    }

    fn end_busy(&self) {
        self.busy.set(false);
        self.generate_btn.set_sensitive(true);
        self.stop_btn.set_sensitive(false);
        self.spinner.stop();
        self.progress.set_visible(false);
    }

    fn cancel_generation(&self) {
        self.cancel.set(true);
        self.gen_id.set(self.gen_id.get() + 1);
        self.end_busy();
        self.set_status("Cancelled. Any in-flight request will be ignored.", false);
    }

    fn set_status(&self, msg: &str, is_error: bool) {
        self.status_label.set_text(msg);
        if is_error {
            self.status_label.add_css_class("error-label");
        } else {
            self.status_label.remove_css_class("error-label");
        }
    }

    fn handle_event(self: &Rc<Self>, ev: GenEvent) {
        match ev {
            GenEvent::Status(s) => {
                // Keep the JSON preview fresh; show the first line as status.
                self.request_view.buffer().set_text(&s);
                let first = s.lines().next().unwrap_or("").to_string();
                self.set_status(&first, false);
            }
            GenEvent::Partial { index, b64, media_type } => {
                if let Ok(bytes) = crate::util::b64_decode(&b64) {
                    if let Ok(tex) = crate::refs::texture_from_bytes(&bytes) {
                        let mt = media_type.unwrap_or_else(|| crate::util::guess_media_type(&bytes).to_string());
                        self.stream_box.set_visible(true);
                        self.stream_preview.set_paintable(Some(&tex));
                        self.set_status(&format!("Received partial image {} ({})", index + 1, mt), false);
                        let fraction = ((index + 1) as f64 / 6.0).min(0.95);
                        self.progress.set_fraction(fraction);
                    }
                }
            }
            GenEvent::Done(result) => {
                self.end_busy();
                self.render_results(*result);
            }
            GenEvent::Error(e) => {
                self.end_busy();
                self.set_status(&format!("Error: {e}"), true);
            }
        }
    }

    fn render_results(self: &Rc<Self>, result: ImageResult) {
        if result.data.is_empty() {
            self.set_status("The API returned no image data.", true);
            return;
        }
        self.stream_box.set_visible(false);
        let stamp = crate::util::now_timestamp();
        let model_short = self
            .model
            .borrow()
            .as_ref()
            .map(|m| m.id.replace('/', "_"))
            .unwrap_or_else(|| "image".into());
        let mut count = 0usize;
        for (i, img) in result.data.iter().enumerate() {
            let bytes = match &img.b64_json {
                Some(b) => match crate::util::b64_decode(b) {
                    Ok(v) => v,
                    Err(e) => {
                        self.set_status(&format!("Could not decode image: {e}"), true);
                        continue;
                    }
                },
                None => continue,
            };
            let media_type = img
                .media_type
                .clone()
                .unwrap_or_else(|| crate::util::guess_media_type(&bytes).to_string());
            let ext = crate::util::extension_for(&media_type);
            let name = format!("{}_{}_ {}.{}", model_short, stamp, i + 1, ext).replace(' ', "-");
            let texture = match crate::refs::texture_from_bytes(&bytes) {
                Ok(t) => t,
                Err(e) => {
                    self.set_status(&format!("Could not display image: {e}"), true);
                    continue;
                }
            };
            let (w, h) = match crate::util::decode_rgba(&bytes) {
                Ok((_, w, h)) => (w, h),
                Err(_) => (0, 0),
            };
            self.add_result_card(GeneratedItem {
                name,
                media_type,
                bytes,
                texture,
                width: w,
                height: h,
            });
            count += 1;
        }
        self.set_status(&format!("Done. {} image(s) returned.", count), false);
        if let Some(u) = &result.usage {
            self.usage_label.set_text(&u.summary());
        } else {
            self.usage_label.set_text("No usage reported.");
        }
    }

    fn add_result_card(self: &Rc<Self>, item: GeneratedItem) {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
        card.add_css_class("result-card");

        let pic = gtk::Picture::new();
        pic.set_paintable(Some(&item.texture));
        pic.set_content_fit(gtk::ContentFit::Contain);
        pic.set_can_shrink(true);
        pic.set_size_request(190, 190);
        pic.set_tooltip_text(Some("Click to open in a viewer"));
        {
            let g = gtk::GestureClick::new();
            let t = self.clone();
            let it = item.clone();
            g.connect_released(move |_, _, _, _| {
                crate::open_viewer(Some(&t.window), &it.name, it.bytes.clone(), it.texture.clone());
            });
            pic.add_controller(g);
        }
        card.append(&pic);

        let info = gtk::Label::new(Some(&format!(
            "{} x {} | {}",
            item.width, item.height, item.media_type
        )));
        info.add_css_class("caption");
        card.append(&info);

        let btns = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        btns.set_halign(gtk::Align::Center);
        let save = gtk::Button::with_label("Save");
        save.add_css_class("flat");
        save.set_tooltip_text(Some("Save this image"));
        {
            let t = self.clone();
            let it = item.clone();
            save.connect_clicked(move |_| t.save_single(&it));
        }
        let copy = gtk::Button::with_label("Copy");
        copy.add_css_class("flat");
        copy.set_tooltip_text(Some("Copy image to clipboard"));
        {
            let it = item.clone();
            copy.connect_clicked(move |btn| {
                let cb = btn.clipboard();
                cb.set_texture(&it.texture);
            });
        }
        btns.append(&save);
        btns.append(&copy);
        card.append(&btns);

        self.results.append(&card);
        self.results_store.borrow_mut().push(item);
    }

    fn save_single(&self, item: &GeneratedItem) {
        let dialog = gtk::FileDialog::builder()
            .title("Save image")
            .modal(true)
            .initial_name(item.name.clone())
            .build();
        let bytes = item.bytes.clone();
        dialog.save(Some(&self.window), gio::Cancellable::NONE, move |res| match res {
            Ok(file) => {
                if let Some(path) = file.path() {
                    if let Err(e) = std::fs::write(&path, &bytes) {
                        eprintln!("save failed: {e}");
                    }
                }
            }
            Err(e) => eprintln!("save dialog: {e}"),
        });
    }

    pub fn save_all(self: &Rc<Self>) {
        let items = self.results_store.borrow().clone();
        if items.is_empty() {
            self.set_status("Nothing to save yet.", true);
            return;
        }
        let dialog = gtk::FileDialog::builder()
            .title("Choose a folder for the generated images")
            .modal(true)
            .build();
        let t = self.clone();
        dialog.select_folder(Some(&self.window), gio::Cancellable::NONE, move |res| match res {
            Ok(folder) => {
                let dir = folder.path().unwrap_or_default();
                let mut saved = 0usize;
                for it in &items {
                    let path = dir.join(&it.name);
                    if std::fs::write(&path, &it.bytes).is_ok() {
                        saved += 1;
                    }
                }
                t.set_status(&format!("Saved {} image(s) to {}", saved, dir.display()), false);
            }
            Err(e) => eprintln!("folder dialog: {e}"),
        });
    }

    fn clear_results(self: &Rc<Self>) {
        while let Some(child) = self.results.first_child() {
            self.results.remove(&child);
        }
        self.results_store.borrow_mut().clear();
        self.stream_box.set_visible(false);
        self.usage_label.set_text("");
        self.set_status("Results cleared.", false);
    }

    pub fn preview_json(self: &Rc<Self>) {
        match self.model.borrow().clone() {
            Some(m) => {
                let prompt = text_of(&self.prompt);
                match self.build_request(&m, &prompt) {
                    Ok(r) => {
                        self.request_view
                            .buffer()
                            .set_text(&serde_json::to_string_pretty(&r.to_json()).unwrap_or_default());
                        self.req_exp.set_expanded(true);
                        self.set_status("Request preview updated.", false);
                    }
                    Err(e) => self.set_status(&e, true),
                }
            }
            None => self.set_status("Select a model first.", true),
        }
    }
}

pub fn pseudo_random_seed() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0));
    (d.as_nanos() as i64) & 0x7fff_ffff
}
