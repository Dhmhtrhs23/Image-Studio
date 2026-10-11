//! Reference-image strip: add from file, URL, clipboard paste, or drag and drop.
use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum RefSource {
    Data { png: Vec<u8> },
    Url(String),
}

#[derive(Clone, Debug)]
pub struct RefEntry {
    pub name: String,
    pub source: RefSource,
    pub texture: Option<gtk::gdk::Texture>,
}

impl RefEntry {
    pub fn to_input_ref(&self) -> crate::api::InputRef {
        match &self.source {
            RefSource::Data { png } => {
                let uri = crate::util::to_data_uri("image/png", png);
                crate::api::InputRef::from_data_uri(uri)
            }
            RefSource::Url(u) => crate::api::InputRef::from_data_uri(u.clone()),
        }
    }
    pub fn data_uri(&self) -> String {
        match &self.source {
            RefSource::Data { png } => crate::util::to_data_uri("image/png", png),
            RefSource::Url(u) => u.clone(),
        }
    }
    pub fn is_url(&self) -> bool {
        matches!(self.source, RefSource::Url(_))
    }
}

pub type ChangeCb = Box<dyn Fn(usize)>;
pub type MessageCb = Box<dyn Fn(String)>;

pub struct RefStrip {
    pub root: gtk::Box,
    pub thumbs: gtk::Box,
    pub count_label: gtk::Label,
    pub add_button: gtk::Button,
    pub paste_button: gtk::Button,
    pub clear_button: gtk::Button,
    pub url_button: gtk::Button,
    pub entries: RefCell<Vec<RefEntry>>,
    /// -1 unlimited, 0 disabled, >0 maximum count.
    pub max: Cell<i64>,
    pub on_change: RefCell<Option<ChangeCb>>,
    pub on_message: RefCell<Option<MessageCb>>,
    pub parent_window: RefCell<Option<gtk::Window>>,
}

impl RefStrip {
    pub fn new(max: i64) -> Rc<RefStrip> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("ref-strip");

        let header = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let header_top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title = gtk::Label::new(Some("Reference images"));
        title.add_css_class("section-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header_top.append(&title);

        let count_label = gtk::Label::new(Some("0 / 0"));
        count_label.add_css_class("dim-label");
        header_top.append(&count_label);
        header.append(&header_top);

        let add_button = gtk::Button::with_label("Add files");
        add_button.set_tooltip_text(Some("Choose one or more reference images (PNG, JPEG, WebP)"));
        let paste_button = gtk::Button::with_label("Paste");
        paste_button.set_tooltip_text(Some("Paste an image from the clipboard (Ctrl+V)"));
        let url_button = gtk::Button::with_label("URL");
        url_button.set_tooltip_text(Some("Add a public http(s) image URL as a reference"));
        let clear_button = gtk::Button::with_label("Clear");
        let header_btns = gtk::FlowBox::new();
        header_btns.set_selection_mode(gtk::SelectionMode::None);
        header_btns.set_max_children_per_line(4);
        header_btns.set_column_spacing(6);
        header_btns.set_row_spacing(4);
        header_btns.set_halign(gtk::Align::Start);
        for b in [&add_button, &paste_button, &url_button, &clear_button] {
            b.add_css_class("flat");
            header_btns.insert(b, -1);
        }
        header.append(&header_btns);
        root.append(&header);

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
        scroller.set_min_content_height(104);
        scroller.set_hexpand(true);
        let thumbs = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        thumbs.set_margin_top(4);
        thumbs.set_margin_bottom(4);
        scroller.set_child(Some(&thumbs));
        root.append(&scroller);

        let strip = Rc::new(RefStrip {
            root,
            thumbs,
            count_label,
            add_button: add_button.clone(),
            paste_button: paste_button.clone(),
            clear_button: clear_button.clone(),
            url_button: url_button.clone(),
            entries: RefCell::new(Vec::new()),
            max: Cell::new(max),
            on_change: RefCell::new(None),
            on_message: RefCell::new(None),
            parent_window: RefCell::new(None),
        });

        {
            let s = strip.clone();
            add_button.connect_clicked(move |_| s.pick_files());
        }
        {
            let s = strip.clone();
            paste_button.connect_clicked(move |btn| s.paste_from(btn));
        }
        {
            let s = strip.clone();
            url_button.connect_clicked(move |btn| s.prompt_url(btn));
        }
        {
            let s = strip.clone();
            clear_button.connect_clicked(move |_| s.clear());
        }

        {
            let s = strip.clone();
            let dt = gtk::DropTarget::new(gtk::gdk::FileList::static_type(), gtk::gdk::DragAction::COPY);
            dt.connect_drop(move |_, value, _, _| {
                if let Ok(list) = value.get::<gtk::gdk::FileList>() {
                    for f in list.files() {
                        let _ = s.add_file(&f);
                    }
                    true
                } else {
                    false
                }
            });
            strip.root.add_controller(dt);
        }
        {
            let s = strip.clone();
            let dt = gtk::DropTarget::new(gtk::gdk::Texture::static_type(), gtk::gdk::DragAction::COPY);
            dt.connect_drop(move |_, value, _, _| {
                if let Ok(tex) = value.get::<gtk::gdk::Texture>() {
                    let png = tex.save_to_png_bytes();
                    let _ = s.add_bytes("dropped image", png.as_ref());
                    true
                } else {
                    false
                }
            });
            strip.root.add_controller(dt);
        }

        strip.notify_change();
        strip
    }

    pub fn set_parent_window(&self, win: &impl IsA<gtk::Window>) {
        *self.parent_window.borrow_mut() = Some(win.clone().upcast::<gtk::Window>());
    }

    pub fn set_on_change(&self, cb: ChangeCb) {
        *self.on_change.borrow_mut() = Some(cb);
    }

    pub fn set_on_message(&self, cb: MessageCb) {
        *self.on_message.borrow_mut() = Some(cb);
    }

    pub fn emit_message(&self, msg: &str) {
        crate::util::debug_log(msg);
        if let Some(cb) = self.on_message.borrow().as_ref() {
            cb(msg.to_string());
        }
    }

    pub fn set_max(&self, max: i64) {
        let changed = self.max.get() != max;
        self.max.set(max);
        if changed {
            self.notify_change();
        }
    }

    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    pub fn clear(self: &Rc<Self>) {
        self.entries.borrow_mut().clear();
        Self::rebuild_thumbnails(self);
        self.notify_change();
    }

    pub fn input_refs(&self) -> Vec<crate::api::InputRef> {
        self.entries.borrow().iter().map(|e| e.to_input_ref()).collect()
    }

    pub fn data_uris(&self) -> Vec<String> {
        self.entries.borrow().iter().map(|e| e.data_uri()).collect()
    }

    pub fn add_bytes(self: &Rc<Self>, name: &str, bytes: &[u8]) -> Result<(), String> {
        if self.max.get() == 0 {
            return Err("The selected model does not advertise reference-image support".into());
        }
        let total = self.entries.borrow().len() as i64;
        if self.max.get() > 0 && total >= self.max.get() {
            return Err(format!("This model accepts at most {} reference image(s)", self.max.get()));
        }
        let png = crate::util::to_png(bytes)?;
        let texture = crate::refs::texture_from_bytes(&png).ok();
        self.entries.borrow_mut().push(RefEntry {
            name: name.to_string(),
            source: RefSource::Data { png },
            texture,
        });
        Self::rebuild_thumbnails(self);
        self.notify_change();
        Ok(())
    }

    pub fn add_url(self: &Rc<Self>, url: &str) -> Result<(), String> {
        let url = url.trim();
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("data:")) {
            return Err("URL must start with http://, https:// or data:".into());
        }
        if self.max.get() == 0 {
            return Err("The selected model does not advertise reference-image support".into());
        }
        let total = self.entries.borrow().len() as i64;
        if self.max.get() > 0 && total >= self.max.get() {
            return Err(format!("This model accepts at most {} reference image(s)", self.max.get()));
        }
        let texture = if url.starts_with("data:") {
            crate::util::b64_decode(url).ok().and_then(|b| texture_from_bytes(&b).ok())
        } else {
            None
        };
        self.entries.borrow_mut().push(RefEntry {
            name: url.chars().take(48).collect(),
            source: RefSource::Url(url.to_string()),
            texture,
        });
        Self::rebuild_thumbnails(self);
        self.notify_change();
        Ok(())
    }

    pub fn add_file(self: &Rc<Self>, file: &gio::File) -> Result<(), String> {
        let (bytes, _etag) = file
            .load_contents(gio::Cancellable::NONE)
            .map_err(|e| format!("read failed: {e}"))?;
        let name = file
            .basename()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".into());
        self.add_bytes(&name, &bytes)
    }

    pub fn add_texture(self: &Rc<Self>, name: &str, tex: &gtk::gdk::Texture) -> Result<(), String> {
        let png = tex.save_to_png_bytes();
        self.add_bytes(name, png.as_ref())
    }

    pub fn pick_files(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Choose reference images")
            .modal(true)
            .build();
        let parent = self.parent_window.borrow().clone();
        let s = self.clone();
        dialog.open_multiple(parent.as_ref(), gio::Cancellable::NONE, move |res| match res {
            Ok(model) => {
                let n = model.n_items();
                for i in 0..n {
                    if let Some(obj) = model.item(i) {
                        if let Ok(file) = obj.downcast::<gio::File>() {
                            if let Err(e) = s.add_file(&file) {
                                eprintln!("add reference: {e}");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if !e.matches(gio::IOErrorEnum::Cancelled) {
                    eprintln!("file dialog: {e}");
                }
            }
        });
    }

    pub fn prompt_url(self: &Rc<Self>, btn: &gtk::Button) {
        let dialog = gtk::Window::builder()
            .title("Add reference URL")
            .modal(true)
            .default_width(520)
            .build();
        if let Some(win) = self.parent_window.borrow().clone().or_else(|| btn.root().and_downcast::<gtk::Window>()) {
            dialog.set_transient_for(Some(&win));
        }
        let bx = gtk::Box::new(gtk::Orientation::Vertical, 8);
        bx.set_margin_top(12);
        bx.set_margin_bottom(12);
        bx.set_margin_start(12);
        bx.set_margin_end(12);
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("https://example.com/image.png"));
        entry.set_hexpand(true);
        bx.append(&entry);
        let err_lbl = gtk::Label::new(None);
        err_lbl.add_css_class("error-label");
        bx.append(&err_lbl);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label("Cancel");
        let ok = gtk::Button::with_label("Add");
        ok.add_css_class("suggested-action");
        row.append(&cancel);
        row.append(&ok);
        bx.append(&row);
        dialog.set_child(Some(&bx));
        {
            let d = dialog.clone();
            cancel.connect_clicked(move |_| d.close());
        }
        {
            let d = dialog.clone();
            let s = self.clone();
            let entry = entry.clone();
            let err_lbl = err_lbl.clone();
            ok.connect_clicked(move |_| match s.add_url(&entry.text()) {
                Ok(()) => d.close(),
                Err(e) => err_lbl.set_text(&e),
            });
        }
        {
            let d = dialog.clone();
            let s = self.clone();
            entry.connect_activate(move |e| {
                if s.add_url(&e.text()).is_ok() {
                    d.close();
                }
            });
        }
        dialog.present();
        entry.grab_focus();
    }

    pub fn paste_from(self: &Rc<Self>, btn: &gtk::Button) {
        self.paste_clipboard(&btn.clipboard());
    }

    pub fn paste_clipboard(self: &Rc<Self>, clipboard: &gtk::gdk::Clipboard) {
        self.emit_message("clipboard: reading image...");
        let clip_tex = clipboard.clone();
        let clip_text = clipboard.clone();
        let s_tex = self.clone();
        let s_text = self.clone();
        clip_tex.read_texture_async(gio::Cancellable::NONE, move |res| match res {
            Ok(Some(tex)) => match s_tex.add_texture("clipboard image", &tex) {
                Ok(()) => s_tex.emit_message("clipboard: image added as reference"),
                Err(e) => s_tex.emit_message(&format!("clipboard: {e}")),
            },
            Ok(None) => {
                s_text.emit_message("clipboard: no bitmap, checking text/URL...");
                let s2 = s_text.clone();
                let cb = clip_text;
                cb.read_text_async(gio::Cancellable::NONE, move |res2| match res2 {
                    Ok(Some(text)) => {
                        let t = text.to_string();
                        if t.starts_with("http") || t.starts_with("data:") {
                            match s2.add_url(&t) {
                                Ok(()) => s2.emit_message("clipboard: URL added as reference"),
                                Err(e) => s2.emit_message(&format!("clipboard: {e}")),
                            }
                        } else {
                            s2.emit_message("clipboard: no image found");
                        }
                    }
                    _ => s2.emit_message("clipboard: no image found"),
                });
            }
            Err(e) => s_tex.emit_message(&format!("clipboard error: {e}")),
        });
    }

    fn notify_change(&self) {
        let n = self.entries.borrow().len();
        let max = self.max.get();
        let max_s = if max < 0 { "unlimited".to_string() } else { max.to_string() };
        self.count_label.set_text(&format!("{} / {}", n, max_s));
        self.update_sensitivity();
        if let Some(cb) = self.on_change.borrow().as_ref() {
            cb(n);
        }
    }

    fn update_sensitivity(&self) {
        let n = self.entries.borrow().len() as i64;
        let max = self.max.get();
        let allowed = max != 0;
        let full = max > 0 && n >= max;
        for b in [&self.add_button, &self.paste_button, &self.url_button] {
            b.set_sensitive(allowed && !full);
        }
        self.clear_button.set_sensitive(n > 0);
        self.root.set_visible(true);
    }

    fn rebuild_thumbnails(self: &Rc<Self>) {
        while let Some(child) = self.thumbs.first_child() {
            self.thumbs.remove(&child);
        }
        let entries = self.entries.borrow();
        if entries.is_empty() {
            let hint = gtk::Label::new(Some(
                "No reference images. Add files, paste with Ctrl+V, or drop images here.",
            ));
            hint.add_css_class("dim-label");
            hint.set_wrap(true);
            hint.set_max_width_chars(36);
            hint.set_margin_top(28);
            hint.set_margin_bottom(28);
            self.thumbs.append(&hint);
            return;
        }
        for (idx, entry) in entries.iter().enumerate() {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
            card.add_css_class("ref-card");
            card.set_size_request(96, -1);
            match &entry.texture {
                Some(tex) => {
                    let pic = gtk::Picture::new();
                    pic.set_content_fit(gtk::ContentFit::Contain);
                    pic.set_can_shrink(true);
                    pic.set_size_request(88, 66);
                    pic.set_paintable(Some(tex));
                    card.append(&pic);
                }
                None => {
                    let placeholder = gtk::Image::from_icon_name("image-x-generic-symbolic");
                    placeholder.set_pixel_size(36);
                    placeholder.set_size_request(88, 66);
                    card.append(&placeholder);
                }
            }
            let name = gtk::Label::new(Some(&ellipsize(&entry.name, 16)));
            name.add_css_class("caption");
            name.set_tooltip_text(Some(&entry.name));
            card.append(&name);
            let remove = gtk::Button::from_icon_name("window-close-symbolic");
            remove.add_css_class("circular");
            remove.add_css_class("flat");
            remove.set_tooltip_text(Some("Remove this reference"));
            remove.set_halign(gtk::Align::End);
            {
                let s = self.clone();
                remove.connect_clicked(move |_| {
                    s.entries.borrow_mut().remove(idx);
                    Self::rebuild_thumbnails(&s);
                    s.notify_change();
                });
            }
            card.append(&remove);
            self.thumbs.append(&card);
        }
    }
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push_str("...");
        out
    }
}

pub fn texture_from_bytes(bytes: &[u8]) -> Result<gtk::gdk::Texture, String> {
    let (rgba, w, h) = crate::util::decode_rgba(bytes)?;
    let bytes = glib::Bytes::from_owned(rgba);
    let tex = gtk::gdk::MemoryTexture::new(
        w as i32,
        h as i32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        (w * 4) as usize,
    );
    Ok(tex.upcast())
}
