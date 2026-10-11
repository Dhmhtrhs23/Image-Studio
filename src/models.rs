//! Model browser sidebar plus a capabilities/pricing panel for the selected model.
use crate::api::{self, Capability, Endpoint, ImageModel};
use crate::state::AppState;
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub type ModelCb = Box<dyn Fn(Option<&ImageModel>)>;
pub type EndpointCb = Box<dyn Fn(&[Endpoint])>;

pub struct ModelPane {
    pub sidebar: gtk::Box,
    pub caps_box: gtk::Box,
    state: Rc<AppState>,
    list: gtk::ListBox,
    search: gtk::SearchEntry,
    filter_dd: gtk::DropDown,
    count_label: gtk::Label,
    visible: RefCell<Vec<ImageModel>>,
    all: RefCell<Vec<ImageModel>>,
    on_model: RefCell<Option<ModelCb>>,
    on_endpoints: RefCell<Option<EndpointCb>>,
    selected_id: RefCell<Option<String>>,
    request_id: Rc<Cell<u64>>,
}

impl ModelPane {
    pub fn new(state: Rc<AppState>) -> Rc<ModelPane> {
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
        sidebar.set_margin_top(8);
        sidebar.set_margin_bottom(8);
        sidebar.set_margin_start(8);
        sidebar.set_margin_end(8);
        sidebar.set_size_request(290, -1);
        sidebar.set_hexpand(false);

        let title = gtk::Label::new(Some("Image models"));
        title.set_xalign(0.0);
        title.add_css_class("section-title");
        sidebar.append(&title);

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search models..."));
        sidebar.append(&search);

        let filter_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let filter_dd = gtk::DropDown::from_strings(&[
            "All models",
            "Accepts reference images",
            "Supports streaming",
        ]);
        filter_dd.set_hexpand(true);
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Reload the model list from OpenRouter"));
        filter_row.append(&filter_dd);
        filter_row.append(&refresh);
        sidebar.append(&filter_row);

        let count_label = gtk::Label::new(Some("No models loaded"));
        count_label.set_xalign(0.0);
        count_label.add_css_class("dim-label");
        sidebar.append(&count_label);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_vexpand(true);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("model-list");
        scroll.set_child(Some(&list));
        sidebar.append(&scroll);

        let caps_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        caps_box.set_margin_top(8);
        caps_box.set_margin_bottom(8);
        caps_box.set_margin_start(8);
        caps_box.set_margin_end(8);
        caps_box.set_valign(gtk::Align::Start);

        let pane = Rc::new(ModelPane {
            sidebar,
            caps_box,
            state,
            list: list.clone(),
            search: search.clone(),
            filter_dd: filter_dd.clone(),
            count_label,
            visible: RefCell::new(Vec::new()),
            all: RefCell::new(Vec::new()),
            on_model: RefCell::new(None),
            on_endpoints: RefCell::new(None),
            selected_id: RefCell::new(None),
            request_id: Rc::new(Cell::new(0)),
        });

        {
            let p = pane.clone();
            refresh.connect_clicked(move |_| p.refresh());
        }
        {
            let p = pane.clone();
            search.connect_search_changed(move |_| p.render_list());
        }
        {
            let p = pane.clone();
            filter_dd.connect_selected_notify(move |_| p.render_list());
        }
        {
            let p = pane.clone();
            list.connect_row_selected(move |_, row| {
                if let Some(row) = row {
                    let idx = row.index();
                    if idx >= 0 {
                        let m = p.visible.borrow().get(idx as usize).cloned();
                        if let Some(m) = m {
                            p.select_model(m, true);
                        }
                    }
                }
            });
        }

        pane.render_caps_placeholder();
        pane
    }

    pub fn set_on_model(&self, cb: ModelCb) {
        *self.on_model.borrow_mut() = Some(cb);
    }

    pub fn set_on_endpoints(&self, cb: EndpointCb) {
        *self.on_endpoints.borrow_mut() = Some(cb);
    }

    pub fn refresh(self: &Rc<Self>) {
        if !self.state.has_key() {
            self.count_label.set_text("Add an API key to load models");
            return;
        }
        self.count_label.set_text("Loading models...");
        let (tx, rx) = async_channel::unbounded::<Result<Vec<ImageModel>, String>>();
        let client = self.state.client();
        let base = self.state.base_url();
        let key = self.state.api_key();
        std::thread::spawn(move || {
            let res = api::fetch_image_models(&client, &base, &key);
            let _ = tx.send_blocking(res);
        });
        let pane = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(res) = rx.recv().await {
                match res {
                    Ok(models) => pane.set_models(models),
                    Err(e) => pane.count_label.set_text(&format!("Load failed: {e}")),
                }
            }
        });
    }

    pub fn set_models(self: &Rc<Self>, models: Vec<ImageModel>) {
        *self.all.borrow_mut() = models;
        self.render_list();
        // Auto-select the last used or the first model.
        let last = self.state.config.borrow().last_model.clone();
        let pick = if !last.is_empty() {
            self.visible
                .borrow()
                .iter()
                .find(|m| m.id == last)
                .cloned()
        } else {
            None
        };
        let pick = pick.or_else(|| self.visible.borrow().first().cloned());
        if let Some(m) = pick {
            self.select_model(m, false);
        }
    }

    fn matches_filter(&self, m: &ImageModel) -> bool {
        let q = self.search.text().to_string().to_lowercase();
        if !q.is_empty() {
            let hay = format!(
                "{} {} {}",
                m.id,
                m.name.clone().unwrap_or_default(),
                m.description.clone().unwrap_or_default()
            )
            .to_lowercase();
            if !hay.contains(&q) {
                return false;
            }
        }
        match self.filter_dd.selected() {
            1 => m.architecture.input_modalities.iter().any(|x| x == "image"),
            2 => m.supports_streaming,
            _ => true,
        }
    }

    fn render_list(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let all = self.all.borrow();
        let filtered: Vec<ImageModel> = all.iter().filter(|m| self.matches_filter(m)).cloned().collect();
        *self.visible.borrow_mut() = filtered.clone();
        if filtered.is_empty() {
            self.count_label
                .set_text(if all.is_empty() { "No models loaded" } else { "No models match the filter" });
        } else {
            self.count_label
                .set_text(&format!("{} of {} models", filtered.len(), all.len()));
        }
        for m in &filtered {
            let row = gtk::ListBoxRow::new();
            let bx = gtk::Box::new(gtk::Orientation::Vertical, 2);
            bx.set_margin_top(6);
            bx.set_margin_bottom(6);
            bx.set_margin_start(8);
            bx.set_margin_end(8);
            let name = gtk::Label::new(Some(&m.display_name()));
            name.set_xalign(0.0);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_max_width_chars(30);
            name.add_css_class("model-name");
            bx.append(&name);
            let id = gtk::Label::new(Some(&m.id));
            id.set_xalign(0.0);
            id.set_ellipsize(gtk::pango::EllipsizeMode::End);
            id.set_max_width_chars(34);
            id.add_css_class("dim-label");
            id.add_css_class("caption");
            bx.append(&id);
            let badges = gtk::Label::new(Some(&badge_text(m)));
            badges.set_xalign(0.0);
            badges.add_css_class("badges");
            badges.set_wrap(true);
            bx.append(&badges);
            row.set_child(Some(&bx));
            self.list.append(&row);
        }
    }

    pub fn select_model(self: &Rc<Self>, model: ImageModel, from_user: bool) {
        *self.selected_id.borrow_mut() = Some(model.id.clone());
        if let Some(cb) = self.on_model.borrow().as_ref() {
            cb(Some(&model));
        }
        self.render_caps(&model, None, true);
        // Fetch endpoint detail for pricing and provider capabilities.
        if self.state.has_key() {
            let (tx, rx) = async_channel::unbounded::<Result<Vec<Endpoint>, String>>();
            let client = self.state.client();
            let base = self.state.base_url();
            let key = self.state.api_key();
            let id = model.id.clone();
            std::thread::spawn(move || {
                let res = api::fetch_endpoints(&client, &base, &key, &id);
                let _ = tx.send_blocking(res);
            });
            let rid = self.request_id.get() + 1;
            self.request_id.set(rid);
            let pane = self.clone();
            let model2 = model.clone();
            glib::spawn_future_local(async move {
                if let Ok(res) = rx.recv().await {
                    if pane.request_id.get() != rid {
                        return;
                    }
                    match res {
                        Ok(eps) => {
                            if let Some(cb) = pane.on_endpoints.borrow().as_ref() {
                                cb(&eps);
                            }
                            pane.render_caps(&model2, Some(&eps), false);
                        }
                        Err(e) => {
                            pane.render_caps(&model2, None, false);
                            eprintln!("endpoints: {e}");
                        }
                    }
                }
            });
        }
        if from_user {
            self.state.config.borrow_mut().last_model = model.id.clone();
        }
    }

    pub fn selected_model_id(&self) -> Option<String> {
        self.selected_id.borrow().clone()
    }

    fn render_caps_placeholder(&self) {
        while let Some(c) = self.caps_box.first_child() {
            self.caps_box.remove(&c);
        }
        let l = gtk::Label::new(Some("Select a model to see its capabilities, providers and pricing."));
        l.add_css_class("dim-label");
        l.set_xalign(0.0);
        self.caps_box.append(&l);
    }

    fn render_caps(&self, model: &ImageModel, endpoints: Option<&[Endpoint]>, loading: bool) {
        while let Some(c) = self.caps_box.first_child() {
            self.caps_box.remove(&c);
        }

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let name = gtk::Label::new(Some(&model.display_name()));
        name.set_xalign(0.0);
        name.add_css_class("model-headline");
        header.append(&name);
        let id = gtk::Label::new(Some(&model.id));
        id.add_css_class("dim-label");
        header.append(&id);
        self.caps_box.append(&header);

        if let Some(d) = &model.description {
            let desc = gtk::Label::new(Some(d));
            desc.set_xalign(0.0);
            desc.set_wrap(true);
            desc.add_css_class("dim-label");
            self.caps_box.append(&desc);
        }

        let badges = gtk::Label::new(Some(&format!(
            "IN: {}   OUT: {}   streaming: {}   max refs: {}   max n: {}",
            model.input_modalities(),
            model.output_modalities(),
            if model.supports_streaming { "yes" } else { "no" },
            if model.max_refs() > 0 { model.max_refs().to_string() } else { "n/a".into() },
            model.max_n()
        )));
        badges.set_xalign(0.0);
        badges.set_wrap(true);
        badges.add_css_class("badges");
        self.caps_box.append(&badges);

        let caps = model.capabilities();
        if !caps.is_empty() {
            let title = gtk::Label::new(Some("Supported parameters"));
            title.set_xalign(0.0);
            title.add_css_class("section-title");
            self.caps_box.append(&title);
            let grid = gtk::Grid::new();
            grid.set_row_spacing(3);
            grid.set_column_spacing(12);
            let mut keys: Vec<&String> = caps.keys().collect();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                let key = gtk::Label::new(Some(k));
                key.set_xalign(0.0);
                key.add_css_class("param-label");
                let val = gtk::Label::new(Some(&cap_desc(caps.get(*k).unwrap())));
                val.set_xalign(0.0);
                val.set_wrap(true);
                grid.attach(&key, 0, i as i32, 1, 1);
                grid.attach(&val, 1, i as i32, 1, 1);
            }
            self.caps_box.append(&grid);
        } else {
            let l = gtk::Label::new(Some("This model advertises no optional parameters (prompt only)."));
            l.set_xalign(0.0);
            l.set_wrap(true);
            l.add_css_class("dim-label");
            self.caps_box.append(&l);
        }

        let title = gtk::Label::new(Some(if loading { "Providers & pricing (loading...)" } else { "Providers & pricing" }));
        title.set_xalign(0.0);
        title.add_css_class("section-title");
        self.caps_box.append(&title);
        match endpoints {
            Some(eps) if !eps.is_empty() => {
                let info = gtk::Label::new(Some(&endpoints_text(eps)));
                info.set_xalign(0.0);
                info.set_wrap(true);
                info.add_css_class("dim-label");
                self.caps_box.append(&info);
            }
            _ => {
                let l = gtk::Label::new(Some(if loading { "Loading endpoint details..." } else { "No endpoint/pricing detail available for this model." }));
                l.set_xalign(0.0);
                l.set_wrap(true);
                l.add_css_class("dim-label");
                self.caps_box.append(&l);
            }
        }
    }
}

fn endpoints_text(eps: &[Endpoint]) -> String {
    let mut out = Vec::new();
    for e in eps {
        let price: Vec<String> = e.pricing.iter().map(|p| p.label()).collect();
        let passthrough = if e.allowed_passthrough_parameters.is_empty() {
            String::new()
        } else {
            format!("\n    passthrough: {}", e.allowed_passthrough_parameters.join(", "))
        };
        out.push(format!(
            "- {} (slug: {}) stream={}\n    {}{}",
            e.label(),
            e.pin_slug().unwrap_or_else(|| "?".into()),
            e.supports_streaming,
            if price.is_empty() { "no pricing listed".into() } else { price.join("\n    ") },
            passthrough
        ));
    }
    out.join("\n")
}

fn badge_text(m: &ImageModel) -> String {
    let mut b: Vec<String> = Vec::new();
    if m.architecture.input_modalities.iter().any(|x| x == "image") {
        b.push("image input".into());
    }
    if m.architecture.input_modalities.iter().any(|x| x == "file") {
        b.push("file input".into());
    }
    if m.supports_streaming {
        b.push("streaming".into());
    }
    if let Some(c) = m.cap("input_references") {
        if c.max.unwrap_or(0) > 0 {
            b.push(format!("refs<= {}", c.max.unwrap()));
        }
    }
    if let Some(r) = m.cap("resolution") {
        if let Some(v) = r.values {
            if !v.is_empty() {
                b.push(format!("res: {}", v.join("/")));
            }
        }
    }
    if let Some(q) = m.cap("quality") {
        if let Some(v) = q.values {
            b.push(format!("quality: {}", v.join("/")));
        }
    }
    if b.is_empty() {
        "prompt only".into()
    } else {
        b.join("  |  ")
    }
}

pub fn cap_desc(c: &Capability) -> String {
    match c.kind.as_str() {
        "enum" => match &c.values {
            Some(v) => format!("one of: {}", v.join(", ")),
            None => "enum".into(),
        },
        "range" => format!("range: {} .. {}", c.min.unwrap_or(0), c.max.unwrap_or(0)),
        "boolean" => "on / off".into(),
        other => {
            if other.is_empty() {
                "supported".into()
            } else {
                other.to_string()
            }
        }
    }
}
