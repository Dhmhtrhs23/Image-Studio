//! Image understanding tab: chat with image inputs via /chat/completions.
use crate::api::{ChatEvent, Usage};
use crate::refs::RefStrip;
use crate::state::AppState;
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

fn text_of(tv: &gtk::TextView) -> String {
    let buf = tv.buffer();
    buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string()
}

pub struct UnderstandTab {
    pub root: gtk::Box,
    state: Rc<AppState>,
    window: gtk::ApplicationWindow,
    pub ref_strip: Rc<RefStrip>,
    model_entry: gtk::Entry,
    prompt: gtk::TextView,
    system_entry: gtk::Entry,
    stream_switch: gtk::Switch,
    max_tokens_adj: gtk::Adjustment,
    max_tokens_spin: gtk::SpinButton,
    send_btn: gtk::Button,
    stop_btn: gtk::Button,
    spinner: gtk::Spinner,
    status_label: gtk::Label,
    usage_label: gtk::Label,
    response: gtk::TextView,
    busy: Rc<Cell<bool>>,
    cancel: Rc<Cell<bool>>,
    chat_id: Rc<Cell<u64>>,
}

impl UnderstandTab {
    pub fn new(state: Rc<AppState>, window: gtk::ApplicationWindow, ref_strip: Rc<RefStrip>) -> Rc<UnderstandTab> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        hbox.set_vexpand(true);

        // LEFT controls
        let left_scroll = gtk::ScrolledWindow::new();
        left_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        left_scroll.set_min_content_width(380);
        let left = gtk::Box::new(gtk::Orientation::Vertical, 10);
        left.set_margin_top(10);
        left.set_margin_bottom(10);
        left.set_margin_start(10);
        left.set_margin_end(10);

        let info = gtk::Label::new(Some(
            "Ask questions about images. Uses the OpenRouter chat completions API with image inputs (vision models).",
        ));
        info.set_xalign(0.0);
        info.set_wrap(true);
        info.add_css_class("dim-label");
        left.append(&info);

        let model_title = gtk::Label::new(Some("Vision model"));
        model_title.set_xalign(0.0);
        model_title.add_css_class("section-title");
        left.append(&model_title);
        let model_entry = gtk::Entry::new();
        model_entry.set_text(&state.config.borrow().understand_model);
        model_entry.set_placeholder_text(Some("e.g. google/gemini-2.5-flash"));
        left.append(&model_entry);

        let prompt_title = gtk::Label::new(Some("Question / instruction"));
        prompt_title.set_xalign(0.0);
        prompt_title.add_css_class("section-title");
        left.append(&prompt_title);
        let prompt = gtk::TextView::new();
        prompt.set_wrap_mode(gtk::WrapMode::WordChar);
        let prompt_scroll = gtk::ScrolledWindow::new();
        prompt_scroll.set_min_content_height(110);
        prompt_scroll.add_css_class("card");
        prompt_scroll.set_child(Some(&prompt));
        left.append(&prompt_scroll);

        left.append(&ref_strip.root);

        let sys_title = gtk::Label::new(Some("System prompt (optional)"));
        sys_title.set_xalign(0.0);
        sys_title.add_css_class("section-title");
        left.append(&sys_title);
        let system_entry = gtk::Entry::new();
        system_entry.set_placeholder_text(Some("You are a helpful image analyst."));
        left.append(&system_entry);

        let opts = gtk::Frame::new(Some("Options"));
        let og = gtk::Box::new(gtk::Orientation::Vertical, 6);
        og.set_margin_top(8);
        og.set_margin_bottom(8);
        og.set_margin_start(8);
        og.set_margin_end(8);
        let stream_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        stream_row.append(&gtk::Label::new(Some("Stream response")));
        let stream_switch = gtk::Switch::new();
        stream_switch.set_active(true);
        stream_row.append(&stream_switch);
        og.append(&stream_row);
        let mt_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        mt_row.append(&gtk::Label::new(Some("Max tokens")));
        let max_tokens_adj = gtk::Adjustment::new(1024.0, 16.0, 32000.0, 16.0, 128.0, 0.0);
        let max_tokens_spin = gtk::SpinButton::new(Some(&max_tokens_adj), 16.0, 0);
        mt_row.append(&max_tokens_spin);
        og.append(&mt_row);
        opts.set_child(Some(&og));
        left.append(&opts);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let send_btn = gtk::Button::with_label("Send");
        send_btn.add_css_class("suggested-action");
        let stop_btn = gtk::Button::with_label("Stop");
        stop_btn.set_sensitive(false);
        let spinner = gtk::Spinner::new();
        actions.append(&send_btn);
        actions.append(&stop_btn);
        actions.append(&spinner);
        left.append(&actions);

        let status_label = gtk::Label::new(Some("Ready."));
        status_label.set_xalign(0.0);
        status_label.set_wrap(true);
        status_label.add_css_class("status-label");
        left.append(&status_label);
        let usage_label = gtk::Label::new(None);
        usage_label.set_xalign(0.0);
        usage_label.add_css_class("dim-label");
        left.append(&usage_label);

        left_scroll.set_size_request(400, -1);
        left_scroll.set_hexpand(false);
        left_scroll.set_child(Some(&left));
        hbox.append(&left_scroll);
        hbox.append(&gtk::Separator::new(gtk::Orientation::Vertical));

        // RIGHT response
        let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
        right.set_margin_top(10);
        right.set_margin_bottom(10);
        right.set_margin_start(10);
        right.set_margin_end(10);
        let resp_title = gtk::Label::new(Some("Response"));
        resp_title.set_xalign(0.0);
        resp_title.add_css_class("section-title");
        right.append(&resp_title);
        let response = gtk::TextView::new();
        response.set_wrap_mode(gtk::WrapMode::WordChar);
        response.set_editable(false);
        response.set_cursor_visible(false);
        response.set_monospace(false);
        let resp_scroll = gtk::ScrolledWindow::new();
        resp_scroll.set_vexpand(true);
        resp_scroll.add_css_class("card");
        resp_scroll.set_child(Some(&response));
        right.append(&resp_scroll);
        right.set_hexpand(true);
        hbox.append(&right);

        root.append(&hbox);

        let tab = Rc::new(UnderstandTab {
            root,
            state,
            window,
            ref_strip,
            model_entry: model_entry.clone(),
            prompt,
            system_entry: system_entry.clone(),
            stream_switch: stream_switch.clone(),
            max_tokens_adj,
            max_tokens_spin,
            send_btn: send_btn.clone(),
            stop_btn: stop_btn.clone(),
            spinner: spinner.clone(),
            status_label,
            usage_label,
            response,
            busy: Rc::new(Cell::new(false)),
            cancel: Rc::new(Cell::new(false)),
            chat_id: Rc::new(Cell::new(0)),
        });

        {
            let t = tab.clone();
            send_btn.connect_clicked(move |_| t.send());
        }
        {
            let t = tab.clone();
            stop_btn.connect_clicked(move |_| t.cancel());
        }
        {
            let t = tab.clone();
            let me = model_entry.clone();
            me.connect_activate(move |_| t.send());
        }
        {
            let t = tab.clone();
            tab.ref_strip.set_on_message(Box::new(move |m| t.set_status(&m, false)));
        }
        tab
    }

    pub fn focus_prompt(&self) {
        self.prompt.grab_focus();
    }

    pub fn send(self: &Rc<Self>) {
        if self.busy.get() {
            return;
        }
        if !self.state.has_key() {
            self.set_status("No API key. Open the Key dialog from the header bar.", true);
            return;
        }
        let model = self.model_entry.text().to_string();
        if model.trim().is_empty() {
            self.set_status("Enter a vision model id.", true);
            return;
        }
        let prompt = text_of(&self.prompt);
        if prompt.trim().is_empty() {
            self.set_status("Enter a question about the images.", true);
            return;
        }
        let mut images = self.ref_strip.data_uris();
        if images.is_empty() {
            self.set_status("Add at least one image to analyse.", true);
            return;
        }
        let sys = self.system_entry.text().to_string();
        let sys_opt = if sys.trim().is_empty() { None } else { Some(sys.trim().to_string()) };
        let messages_prompt = prompt.clone();
        // Remove immediate duplicates but keep order.
        images.dedup();

        self.state.config.borrow_mut().understand_model = model.clone();
        self.state.save();

        let stream = self.stream_switch.is_active();
        let max_tokens = Some(self.max_tokens_spin.value_as_int().max(1) as u32);
        self.response.buffer().set_text("");

        self.busy.set(true);
        self.send_btn.set_sensitive(false);
        self.stop_btn.set_sensitive(true);
        self.spinner.start();
        self.set_status("Sending...", false);
        self.usage_label.set_text("");

        let (tx, rx) = async_channel::unbounded::<ChatEvent>();
        let client = self.state.client();
        let base = self.state.base_url();
        let key = self.state.api_key();
        std::thread::spawn(move || {
            let mut emit = |ev: ChatEvent| {
                let _ = tx.send_blocking(ev);
            };
            crate::api::chat_with_images(
                &client,
                &base,
                &key,
                model.trim(),
                &messages_prompt,
                &images,
                sys_opt.as_deref(),
                stream,
                max_tokens,
                &mut emit,
            );
        });

        let id = self.chat_id.get() + 1;
        self.chat_id.set(id);
        self.cancel.set(false);

        let tab = self.clone();
        glib::spawn_future_local(async move {
            while let Ok(ev) = rx.recv().await {
                if tab.chat_id.get() != id {
                    continue;
                }
                let finished = matches!(ev, ChatEvent::Done { .. } | ChatEvent::Error(_));
                tab.handle(ev);
                if finished {
                    break;
                }
            }
        });
    }

    fn handle(&self, ev: ChatEvent) {
        match ev {
            ChatEvent::Status(s) => self.set_status(&s, false),
            ChatEvent::Delta(d) => {
                let buf = self.response.buffer();
                let mut end = buf.end_iter();
                buf.insert(&mut end, &d);
            }
            ChatEvent::Done { text, usage } => {
                if !text.is_empty() && text_of(&self.response).is_empty() {
                    self.response.buffer().set_text(&text);
                }
                self.end_busy();
                self.set_status("Done.", false);
                self.set_usage(usage);
            }
            ChatEvent::Error(e) => {
                self.end_busy();
                self.set_status(&format!("Error: {e}"), true);
            }
        }
    }

    fn set_usage(&self, usage: Option<Usage>) {
        match usage {
            Some(u) => self.usage_label.set_text(&u.summary()),
            None => self.usage_label.set_text("No usage reported."),
        }
    }

    fn cancel(&self) {
        self.cancel.set(true);
        self.chat_id.set(self.chat_id.get() + 1);
        self.end_busy();
        self.set_status("Cancelled.", false);
    }

    fn end_busy(&self) {
        self.busy.set(false);
        self.send_btn.set_sensitive(true);
        self.stop_btn.set_sensitive(false);
        self.spinner.stop();
    }

    fn set_status(&self, msg: &str, is_error: bool) {
        self.status_label.set_text(msg);
        if is_error {
            self.status_label.add_css_class("error-label");
        } else {
            self.status_label.remove_css_class("error-label");
        }
    }
}
