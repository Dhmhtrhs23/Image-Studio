//! OpenRouter Image Studio - a GTK4 desktop client for the OpenRouter Image API.
mod api;
mod generate;
mod models;
mod refs;
mod state;
mod understand;
mod util;

use api::{GenEvent, GenRequest, InputRef};
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{gio, Application, ApplicationWindow};
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "io.github.openrouter.ImageStudio";
const VERSION: &str = "0.1.0";

const CSS: &str = "
.section-title { font-weight: bold; }
.model-headline { font-weight: bold; font-size: 15px; }
.model-name { font-weight: 600; }
.dim-label { opacity: 0.62; }
.badges { font-size: 11px; opacity: 0.82; }
.caption { font-size: 11px; opacity: 0.75; }
.param-label { opacity: 0.9; }
.status-label { font-family: monospace; font-size: 11px; opacity: 0.9; }
.error-label { color: #c01c28; }
.ref-card { border: 1px solid alpha(currentColor, 0.18); border-radius: 8px; padding: 4px; }
.result-card { border: 1px solid alpha(currentColor, 0.18); border-radius: 10px; padding: 6px; }
.model-list row { border-radius: 8px; }
.caps-frame { border-radius: 10px; }
";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "--list-models" {
        std::process::exit(cli_list_models());
    }
    if args.len() >= 3 && args[1] == "--generate" {
        std::process::exit(cli_generate(&args[2..]));
    }

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    let code = app.run();
    std::process::exit(code.value());
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn build_ui(app: &Application) {
    install_css();
    gtk::Window::set_default_icon_name("openrouter-image-studio");
    let state = Rc::new(state::AppState::new());

    let window = ApplicationWindow::builder()
        .application(app)
        .title("OpenRouter Image Studio")
        .default_width(1280)
        .default_height(860)
        .build();

    // --- reference strips (separate for each tab) ---
    let gen_refs = refs::RefStrip::new(0);
    let und_refs = refs::RefStrip::new(-1);
    gen_refs.set_parent_window(&window);
    und_refs.set_parent_window(&window);

    let gen_tab = generate::GenTab::new(state.clone(), window.clone(), gen_refs.clone());
    let und_tab = understand::UnderstandTab::new(state.clone(), window.clone(), und_refs.clone());
    let model_pane = models::ModelPane::new(state.clone());

    // --- header bar ---
    let header = gtk::HeaderBar::new();
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let title = gtk::Label::new(Some("OpenRouter Image Studio"));
    title.add_css_class("title");
    let subtitle = gtk::Label::new(Some(&key_subtitle(&state)));
    subtitle.add_css_class("subtitle");
    subtitle.add_css_class("dim-label");
    title_box.append(&title);
    title_box.append(&subtitle);
    header.set_title_widget(Some(&title_box));

    let refresh_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh_btn.set_tooltip_text(Some("Reload models (Ctrl+R)"));
    let key_btn = gtk::Button::from_icon_name("dialog-password-symbolic");
    key_btn.set_tooltip_text(Some("API key and settings"));
    let about_btn = gtk::Button::from_icon_name("help-about-symbolic");
    about_btn.set_tooltip_text(Some("About"));
    header.pack_start(&refresh_btn);
    header.pack_start(&key_btn);
    header.pack_end(&about_btn);
    window.set_titlebar(Some(&header));

    // --- capabilities panel above the tabs ---
    let caps_scroll = gtk::ScrolledWindow::new();
    caps_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    caps_scroll.set_min_content_height(120);
    caps_scroll.set_max_content_height(300);
    caps_scroll.set_propagate_natural_height(true);
    caps_scroll.set_child(Some(&model_pane.caps_box));
    let caps_frame = gtk::Frame::new(Some("Model capabilities"));
    caps_frame.add_css_class("caps-frame");
    caps_frame.set_child(Some(&caps_scroll));

    let notebook = gtk::Notebook::new();
    notebook.set_vexpand(true);
    let gen_label = gtk::Label::new(Some("Generate"));
    let und_label = gtk::Label::new(Some("Understand"));
    notebook.append_page(&gen_tab.root, Some(&gen_label));
    notebook.append_page(&und_tab.root, Some(&und_label));

    let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
    right.append(&caps_frame);
    right.append(&notebook);

    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_position(300);
    paned.set_shrink_start_child(true);
    paned.set_resize_start_child(false);
    paned.set_start_child(Some(&model_pane.sidebar));
    paned.set_end_child(Some(&right));
    window.set_child(Some(&paned));

    // --- callbacks ---
    {
        let g = gen_tab.clone();
        model_pane.set_on_model(Box::new(move |m| g.set_model(m)));
    }
    {
        let g = gen_tab.clone();
        model_pane.set_on_endpoints(Box::new(move |eps| g.set_endpoints(eps)));
    }
    {
        let pane = model_pane.clone();
        refresh_btn.connect_clicked(move |_| pane.refresh());
    }
    {
        let st = state.clone();
        let sub = subtitle.clone();
        let pane = model_pane.clone();
        let win = window.clone();
        key_btn.connect_clicked(move |_| {
            let on_saved: Rc<dyn Fn()> = {
                let st2 = st.clone();
                let sub2 = sub.clone();
                let pane2 = pane.clone();
                Rc::new(move || {
                    sub2.set_text(&key_subtitle(&st2));
                    pane2.refresh();
                })
            };
            open_settings(&win, st.clone(), on_saved);
        });
    }
    {
        let win = window.clone();
        about_btn.connect_clicked(move |_| open_about(&win));
    }

    // Ctrl+R refresh, Ctrl+V paste reference into the active tab.
    let keyctl = gtk::EventControllerKey::new();
    keyctl.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let pane = model_pane.clone();
        let gen_refs = gen_refs.clone();
        let und_refs = und_refs.clone();
        let nb = notebook.clone();
        let win = window.clone();
        keyctl.connect_key_pressed(move |_, keyval, _, state_mod| {
            let ctrl = state_mod.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            if !ctrl {
                return glib::Propagation::Proceed;
            }
            match keyval {
                gtk::gdk::Key::r | gtk::gdk::Key::R => {
                    pane.refresh();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::v | gtk::gdk::Key::V => {
                    let clipboard = win.clipboard();
                    if nb.current_page() == Some(0) {
                        gen_refs.paste_clipboard(&clipboard);
                    } else {
                        und_refs.paste_clipboard(&clipboard);
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
    }
    window.add_controller(keyctl);

    // --- initial load ---
    if state.has_key() {
        model_pane.refresh();
    } else {
        let st = state.clone();
        let pane = model_pane.clone();
        let sub = subtitle.clone();
        let win = window.clone();
        glib::idle_add_local_once(move || {
            let on_saved: Rc<dyn Fn()> = {
                let st2 = st.clone();
                let pane2 = pane.clone();
                let sub2 = sub.clone();
                Rc::new(move || {
                    sub2.set_text(&key_subtitle(&st2));
                    pane2.refresh();
                })
            };
            open_settings(&win, st, on_saved);
        });
    }

    window.present();
    gen_tab.focus_prompt();
}

fn key_subtitle(state: &Rc<state::AppState>) -> String {
    if state.has_key() {
        let key = state.api_key();
        let shown = if key.len() > 12 {
            format!("{}...{}", &key[..8], &key[key.len() - 4..])
        } else {
            "set".to_string()
        };
        format!("key {} | {}", shown, state.base_url())
    } else {
        "No API key - click the key icon to add one".to_string()
    }
}

fn open_settings(parent: &ApplicationWindow, state: Rc<state::AppState>, on_saved: Rc<dyn Fn()>) {
    let win = gtk::Window::builder()
        .title("Settings")
        .modal(true)
        .transient_for(parent)
        .default_width(560)
        .build();

    let bx = gtk::Box::new(gtk::Orientation::Vertical, 10);
    bx.set_margin_top(14);
    bx.set_margin_bottom(14);
    bx.set_margin_start(14);
    bx.set_margin_end(14);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(12);

    let key_entry = gtk::PasswordEntry::new();
    key_entry.set_text(&state.api_key());
    key_entry.set_hexpand(true);
    key_entry.set_show_peek_icon(true);
    let base_entry = gtk::Entry::new();
    base_entry.set_text(&state.base_url());
    let user_entry = gtk::Entry::new();
    user_entry.set_text(&state.config.borrow().user);
    let timeout_adj = gtk::Adjustment::new(
        state.config.borrow().timeout_secs as f64,
        30.0,
        3600.0,
        30.0,
        300.0,
        0.0,
    );
    let timeout_spin = gtk::SpinButton::new(Some(&timeout_adj), 30.0, 0);

    let rows: Vec<(&str, gtk::Widget)> = vec![
        ("OpenRouter API key", key_entry.clone().upcast()),
        ("Base URL", base_entry.clone().upcast()),
        ("End-user id (optional)", user_entry.clone().upcast()),
        ("Request timeout (s)", timeout_spin.clone().upcast()),
    ];
    for (i, (name, w)) in rows.iter().enumerate() {
        let l = gtk::Label::new(Some(name));
        l.set_xalign(0.0);
        grid.attach(&l, 0, i as i32, 1, 1);
        grid.attach(w, 1, i as i32, 1, 1);
    }
    bx.append(&grid);

    let hint = gtk::Label::new(Some(
        "Get a key at openrouter.ai/keys. It is stored locally in your config directory with 0600 permissions. The OPENROUTER_API_KEY environment variable overrides the stored key.",
    ));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    hint.add_css_class("dim-label");
    bx.append(&hint);

    let test_label = gtk::Label::new(None);
    test_label.set_xalign(0.0);
    test_label.set_wrap(true);
    bx.append(&test_label);

    let btn_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    btn_row.set_halign(gtk::Align::End);
    let test_btn = gtk::Button::with_label("Test connection");
    let cancel_btn = gtk::Button::with_label("Cancel");
    let save_btn = gtk::Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    btn_row.append(&test_btn);
    btn_row.append(&cancel_btn);
    btn_row.append(&save_btn);
    bx.append(&btn_row);
    win.set_child(Some(&bx));

    {
        let d = win.clone();
        cancel_btn.connect_clicked(move |_| d.close());
    }
    {
        let st = state.clone();
        let tl = test_label.clone();
        let ke = key_entry.clone();
        let be = base_entry.clone();
        test_btn.connect_clicked(move |btn| {
            tl.remove_css_class("error-label");
            tl.set_text("Testing...");
            let key = ke.text().to_string();
            let base = be.text().to_string();
            let client = st.client();
            let tl2 = tl.clone();
            let btn = btn.clone();
            let (tx, rx) = async_channel::unbounded::<Result<api::KeyInfo, String>>();
            std::thread::spawn(move || {
                let _ = tx.send_blocking(api::fetch_key_info(&client, &base, &key));
            });
            btn.set_sensitive(false);
            glib::spawn_future_local(async move {
                if let Ok(res) = rx.recv().await {
                    btn.set_sensitive(true);
                    match res {
                        Ok(info) => {
                            let remaining = info
                                .limit_remaining
                                .map(|v| format!("{:.4}", v))
                                .unwrap_or_else(|| "n/a".into());
                            let used = info.usage.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "n/a".into());
                            let label = info.label.unwrap_or_default();
                            tl2.set_text(&format!(
                                "OK - key {} | used USD {} | remaining USD {}",
                                label, used, remaining
                            ));
                        }
                        Err(e) => {
                            tl2.add_css_class("error-label");
                            tl2.set_text(&format!("Failed: {e}"));
                        }
                    }
                }
            });
        });
    }
    {
        let st = state.clone();
        let d = win.clone();
        let saved = on_saved.clone();
        let ke = key_entry.clone();
        let be = base_entry.clone();
        let ue = user_entry.clone();
        let ta = timeout_adj.clone();
        save_btn.connect_clicked(move |_| {
            let key = ke.text().to_string();
            let base = be.text().to_string();
            {
                let mut cfg = st.config.borrow_mut();
                cfg.api_key = key.trim().to_string();
                if !base.trim().is_empty() {
                    cfg.base_url = base.trim().to_string();
                }
                cfg.user = ue.text().to_string();
                cfg.timeout_secs = ta.value() as u64;
            }
            st.rebuild_client();
            st.save();
            saved();
            d.close();
        });
    }

    win.present();
    key_entry.grab_focus();
}

fn open_about(parent: &ApplicationWindow) {
    let win = gtk::Window::builder()
        .title("About OpenRouter Image Studio")
        .modal(true)
        .transient_for(parent)
        .default_width(520)
        .build();
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 10);
    bx.set_margin_top(16);
    bx.set_margin_bottom(16);
    bx.set_margin_start(16);
    bx.set_margin_end(16);
    let title = gtk::Label::new(Some("OpenRouter Image Studio"));
    title.add_css_class("title-1");
    bx.append(&title);
    let body = gtk::Label::new(Some(&format!(
        "Version {}\n\nA GTK4 desktop client for the OpenRouter Image API.\n\n- Browse image models and inspect per-model capabilities, providers and pricing\n- Generate images with resolution, aspect ratio, size, quality, output format, background, compression, seed, n and streaming\n- Multiple reference images via file picker, drag and drop, clipboard paste (Ctrl+V) or URL\n- Provider routing: only / order / ignore / sort / allow_fallbacks and provider options\n- Image understanding (vision chat) over chat completions\n\nAPI docs: https://openrouter.ai/docs/guides/overview/multimodal/image-generation",
        VERSION
    )));
    body.set_wrap(true);
    body.set_xalign(0.0);
    body.set_selectable(true);
    bx.append(&body);
    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    {
        let d = win.clone();
        close.connect_clicked(move |_| d.close());
    }
    bx.append(&close);
    win.set_child(Some(&bx));
    win.present();
}

pub fn open_viewer(parent: Option<&impl IsA<gtk::Window>>, title: &str, bytes: Vec<u8>, texture: gtk::gdk::Texture) {
    let win = gtk::Window::builder()
        .title(title)
        .default_width(1000)
        .default_height(760)
        .build();
    if let Some(p) = parent {
        win.set_transient_for(Some(p));
    }
    let hb = gtk::HeaderBar::new();
    let save_btn = gtk::Button::with_label("Save as...");
    let bytes2 = bytes.clone();
    let name = title.to_string();
    let w2 = win.clone();
    save_btn.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::builder()
            .title("Save image")
            .modal(true)
            .initial_name(name.clone())
            .build();
        let b = bytes2.clone();
        dialog.save(Some(&w2), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    let _ = std::fs::write(&path, &b);
                }
            }
        });
    });
    hb.pack_end(&save_btn);
    win.set_titlebar(Some(&hb));
    let pic = gtk::Picture::new();
    pic.set_paintable(Some(&texture));
    pic.set_content_fit(gtk::ContentFit::Contain);
    pic.set_can_shrink(true);
    let sc = gtk::ScrolledWindow::new();
    sc.set_child(Some(&pic));
    win.set_child(Some(&sc));
    win.present();
}

// ---------------------------------------------------------------------------
// Headless probes (used for automated verification)
// ---------------------------------------------------------------------------

fn cli_list_models() -> i32 {
    let st = state::AppState::new();
    if !st.has_key() {
        eprintln!("No API key. Set OPENROUTER_API_KEY.");
        return 2;
    }
    let client = st.client();
    match api::fetch_image_models(&client, &st.base_url(), &st.api_key()) {
        Ok(models) => {
            println!("{} image models", models.len());
            for m in models {
                println!(
                    "{} | {} -> {} | stream={} | refs={} | params={}",
                    m.id,
                    m.input_modalities(),
                    m.output_modalities(),
                    m.supports_streaming,
                    if m.max_refs() > 0 { m.max_refs().to_string() } else { "-".into() },
                    m.capabilities().keys().cloned().collect::<Vec<_>>().join(",")
                );
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn cli_generate(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("usage: --generate <model> <prompt> [--out DIR] [--refs FILE1,FILE2] [--stream] [--n N]");
        return 2;
    }
    let model = args[0].clone();
    let mut prompt = String::new();
    let mut refs: Vec<String> = Vec::new();
    let mut stream = false;
    let mut n: Option<u32> = None;
    let mut out_dir = std::path::PathBuf::from("/tmp/openrouter-studio-test");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--refs" if i + 1 < args.len() => {
                refs = args[i + 1].split(',').map(|s| s.trim().to_string()).collect();
                i += 2;
            }
            "--out" if i + 1 < args.len() => {
                out_dir = std::path::PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--n" if i + 1 < args.len() => {
                n = args[i + 1].parse().ok();
                i += 2;
            }
            "--stream" => {
                stream = true;
                i += 1;
            }
            other => {
                if !prompt.is_empty() {
                    prompt.push(' ');
                }
                prompt.push_str(other);
                i += 1;
            }
        }
    }
    if prompt.is_empty() {
        eprintln!("error: empty prompt");
        return 2;
    }
    let st = state::AppState::new();
    if !st.has_key() {
        eprintln!("No API key. Set OPENROUTER_API_KEY.");
        return 2;
    }
    let mut input_refs: Vec<InputRef> = Vec::new();
    for path in &refs {
        match std::fs::read(path) {
            Ok(bytes) => {
                let png = match util::to_png(&bytes) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("ref {path}: {e}");
                        return 1;
                    }
                };
                input_refs.push(InputRef::from_data_uri(util::to_data_uri("image/png", &png)));
            }
            Err(e) => {
                eprintln!("cannot read {path}: {e}");
                return 1;
            }
        }
    }
    let req = GenRequest {
        model: model.clone(),
        prompt: prompt.clone(),
        n,
        input_references: if input_refs.is_empty() { None } else { Some(input_refs) },
        ..Default::default()
    };
    let _ = std::fs::create_dir_all(&out_dir);
    let client = st.client();
    let refcell = RefCell::new(Vec::<String>::new());
    let mut emit = |ev: GenEvent| match ev {
        GenEvent::Status(s) => println!("[status] {}", s.lines().next().unwrap_or("")),
        GenEvent::Partial { index, b64, .. } => {
            println!("[partial] index={} bytes~{}", index, b64.len());
        }
        GenEvent::Error(e) => println!("[error] {e}"),
        GenEvent::Done(r) => {
            println!("[done] images={} usage={}", r.data.len(), r.usage.as_ref().map(|u| u.summary()).unwrap_or_default());
            for (idx, img) in r.data.iter().enumerate() {
                if let Some(b64) = &img.b64_json {
                    if let Ok(bytes) = util::b64_decode(b64) {
                        let mt = img.media_type.clone().unwrap_or_else(|| util::guess_media_type(&bytes).to_string());
                        let ext = util::extension_for(&mt);
                        let path = out_dir.join(format!("out_{}_{}.{}", util::now_timestamp(), idx + 1, ext));
                        let _ = std::fs::write(&path, &bytes);
                        println!("[saved] {} ({})", path.display(), mt);
                        refcell.borrow_mut().push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    };
    api::generate(&client, &st.base_url(), &st.api_key(), &req, stream, &mut emit);
    if refcell.borrow().is_empty() {
        1
    } else {
        0
    }
}
