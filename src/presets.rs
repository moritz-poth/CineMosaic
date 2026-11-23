//! Presets: the look of a sheet under a name (paper, format, frame line, headline style and place, export format –
//! not the source and not the headline text), kept in presets.json in Application Support. `preset_json` turns the
//! window's fields into such a look, `apply_preset` sets them from one; projects keep their look the same way.

use super::*;

thread_local! {
    /// the presets by name, sorted; loaded at start, written to presets.json after every change
    pub static PRESETS: RefCell<Vec<(String, serde_json::Value)>> = const { RefCell::new(Vec::new()) };
}

pub fn presets_path() -> Option<PathBuf> {
    app_dir().map(|d| d.join("presets.json"))
}

/// Reads presets.json (a missing or broken file = no presets) and fills the menu.
pub fn load_presets() {
    let list: Vec<(String, serde_json::Value)> = presets_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Vec<serde_json::Value>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| Some((e.get("name")?.as_str()?.to_string(), e.get("values")?.clone())))
        .collect();
    PRESETS.with(|p| *p.borrow_mut() = list);
}

/// The preset menu in every window.
// shortcut: another window's chosen preset index is not moved when a preset is added or deleted elsewhere
pub fn preset_menu(_app: &AppWindow) {
    let model: Vec<SharedString> = PRESETS.with(|p| p.borrow().iter().map(|e| e.0.as_str().into()).collect());
    for a in all_windows() {
        a.set_presets(ModelRc::new(VecModel::from(model.clone())));
    }
}

/// Writes all presets atomically and refreshes the menu; `status` is the line under it when saving works.
pub fn store_presets(app: &AppWindow, status: String) {
    preset_menu(app);
    let list: Vec<serde_json::Value> = PRESETS.with(|p| p.borrow().iter().map(|(n, v)| serde_json::json!({"name": n, "values": v})).collect());
    let result = presets_path().ok_or_else(|| std::io::Error::other("no home folder")).and_then(|path| {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let part = path.with_extension("json.part");
        std::fs::write(&part, serde_json::to_vec_pretty(&list).unwrap_or_default())?;
        std::fs::rename(&part, &path)
    });
    match result {
        Ok(()) => app.set_preset_status(status.into()),
        Err(e) => {
            log::write(&format!("presets not saved: {e}"));
            app.set_preset_status(tr(["Could not save the presets: {}", "Presets ließen sich nicht sichern: {}", "Impossible d'enregistrer les préréglages : {}", "No se pudieron guardar los preajustes: {}"]).replacen("{}", &e.to_string(), 1).into());
        }
    }
}

/// Everything about the look of the sheet, as JSON: paper, picture format, line, headline style and place, crop
/// marks, export format. Not the source and not the headline text: they belong to the project. The font is kept by
/// name, so the preset works on another Mac.
pub fn preset_json(app: &AppWindow) -> serde_json::Value {
    serde_json::json!({
        "px-units": app.get_px_units(), "paper": app.get_paper(), "custom-w": app.get_custom_w().as_str(), "custom-h": app.get_custom_h().as_str(),
        "landscape": app.get_landscape(), "dpi": app.get_dpi().as_str(), "margin": app.get_margin().as_str(),
        "gap": app.get_gap().as_str(), "auto-cols": app.get_auto_cols(), "cols": app.get_cols().as_str(),
        "picture": app.get_picture(), "aspect": app.get_aspect().as_str(), "trim": app.get_trim(), "bg-color": app.get_bg_color().as_str(),
        "line-on": app.get_line_on(), "line-width": app.get_line_width().as_str(), "line-inset": app.get_line_inset().as_str(),
        "line-color": app.get_line_color().as_str(), "line-pattern": app.get_line_pattern(),
        "texts": text_styles(app), "global-angle": app.get_global_angle().as_str(),
        "mosaic-on": app.get_mosaic_on(), "mosaic-cols": app.get_mosaic_cols().as_str(), "mosaic-frames": app.get_mosaic_frames().as_str(),
        "mosaic-repeat": app.get_mosaic_repeat().as_str(), "mosaic-tint": app.get_mosaic_tint().as_str(), "mosaic-overlay": app.get_mosaic_overlay().as_str(),
        "export-format": app.get_export_format(),
    })
}

/// Style and place of the open text element (the fields of the Text tab), without its words; the font by name.
pub fn text_style(app: &AppWindow) -> serde_json::Value {
    let font = FONTS.with(|f| f.borrow().get(app.get_font() as usize).cloned()).unwrap_or_default();
    serde_json::json!({
        "font": font, "head-size": app.get_head_size().as_str(), "head-bold": app.get_head_bold(), "head-italic": app.get_head_italic(),
        "head-color": app.get_head_color().as_str(), "head-anchor": app.get_head_anchor(),
        "head-dx": app.get_head_dx().as_str(), "head-dy": app.get_head_dy().as_str(),
        "head-pad": [app.get_head_pad_t().as_str(), app.get_head_pad_r().as_str(), app.get_head_pad_b().as_str(), app.get_head_pad_l().as_str()],
        "head-wrap": app.get_head_wrap(), "head-lines": app.get_head_lines(),
        "sh-on": app.get_sh_on(), "sh-blend": app.get_sh_blend(), "sh-color": app.get_sh_color().as_str(), "sh-opacity": app.get_sh_opacity().as_str(),
        "sh-angle": app.get_sh_angle().as_str(), "sh-global": app.get_sh_global(), "sh-distance": app.get_sh_distance().as_str(),
        "sh-spread": app.get_sh_spread().as_str(), "sh-size": app.get_sh_size().as_str(), "sh-contour": app.get_sh_contour(),
        "sh-aa": app.get_sh_aa(), "sh-noise": app.get_sh_noise().as_str(), "sh-knock": app.get_sh_knock(),
    })
}

/// The styles of all text elements, the open one as its fields show it now.
pub fn text_styles(app: &AppWindow) -> Vec<serde_json::Value> {
    let open = text_style(app);
    with_state(app, |st| {
        let st = st.borrow();
        st.texts.iter().enumerate().map(|(k, t)| if k == st.text_open { open.clone() } else { t.1.clone() }).collect()
    })
}

/// Keys of a text style; an old preset has them at the top level (one headline).
fn is_text_key(k: &str) -> bool {
    k == "font" || k.starts_with("head-") || k.starts_with("sh-")
}

/// Sets the Text tab fields from a style. Missing or wrong keys keep the field as it is.
pub fn apply_text_style(app: &AppWindow, v: &serde_json::Value) {
    let int = |k: &str, max: i64, set: &dyn Fn(i32)| {
        if let Some(n) = v.get(k).and_then(|x| x.as_i64()).filter(|n| (0..=max).contains(n)) {
            set(n as i32)
        }
    };
    let boolean = |k: &str, set: &dyn Fn(bool)| v.get(k).and_then(|x| x.as_bool()).into_iter().for_each(set);
    let text = |k: &str, set: &dyn Fn(SharedString)| v.get(k).and_then(|x| x.as_str()).filter(|s| s.len() < 100).into_iter().for_each(|s| set(s.into()));
    if let Some(name) = v.get("font").and_then(|x| x.as_str()).filter(|n| !n.is_empty()) {
        with_state(app, |st| st.borrow_mut().font_wanted = Some(name.to_string()));
        apply_font(app);
    }
    text("head-size", &|s| app.set_head_size(s));
    boolean("head-bold", &|b| app.set_head_bold(b));
    boolean("head-italic", &|b| app.set_head_italic(b));
    text("head-color", &|s| app.set_head_color(s));
    int("head-anchor", 8, &|n| app.set_head_anchor(n));
    text("head-dx", &|s| app.set_head_dx(s));
    text("head-dy", &|s| app.set_head_dy(s));
    if let Some(p) = v.get("head-pad").and_then(|x| x.as_array()).filter(|p| p.len() == 4) {
        let sets: [fn(&AppWindow, SharedString); 4] = [AppWindow::set_head_pad_t, AppWindow::set_head_pad_r, AppWindow::set_head_pad_b, AppWindow::set_head_pad_l];
        for (x, set) in p.iter().zip(sets) {
            if let Some(t) = x.as_str().filter(|t| t.len() < 10) {
                set(app, t.into());
            }
        }
    }
    boolean("head-wrap", &|b| app.set_head_wrap(b));
    int("head-lines", 2, &|n| app.set_head_lines(n));
    boolean("sh-on", &|b| app.set_sh_on(b));
    int("sh-blend", layout::Blend::ALL.len() as i64 - 1, &|n| app.set_sh_blend(n));
    text("sh-color", &|s| app.set_sh_color(s));
    text("sh-opacity", &|s| app.set_sh_opacity(s));
    text("sh-angle", &|s| app.set_sh_angle(s));
    boolean("sh-global", &|b| app.set_sh_global(b));
    text("sh-distance", &|s| app.set_sh_distance(s));
    text("sh-spread", &|s| app.set_sh_spread(s));
    text("sh-size", &|s| app.set_sh_size(s));
    int("sh-contour", layout::Contour::ALL.len() as i64 - 1, &|n| app.set_sh_contour(n));
    boolean("sh-aa", &|b| app.set_sh_aa(b));
    text("sh-noise", &|s| app.set_sh_noise(s));
    boolean("sh-knock", &|b| app.set_sh_knock(b));
    match_slots(app);
}

/// Styles from a preset or project look: by place onto the texts there are (more styles add empty texts, texts
/// beyond them keep theirs); a style only changes the keys it has. Old looks hold one style at the top level.
fn apply_text_styles(app: &AppWindow, v: &serde_json::Value) {
    let styles: Vec<serde_json::Value> = match v.get("texts").and_then(|x| x.as_array()) {
        Some(a) => a.clone(),
        None => {
            let flat: serde_json::Map<String, serde_json::Value> = v.as_object().into_iter().flatten().filter(|(k, _)| is_text_key(k)).map(|(k, x)| (k.clone(), x.clone())).collect();
            if flat.is_empty() { vec![] } else { vec![flat.into()] }
        }
    };
    store_open(app);
    with_state(app, |st| {
        let mut st = st.borrow_mut();
        for (k, style) in styles.into_iter().enumerate() {
            let Some(new) = style.as_object() else { continue };
            if k >= st.texts.len() {
                let base = st.texts.last().map(|t| t.1.clone()).unwrap_or_default();
                st.texts.push((String::new(), base));
            }
            if let Some(old) = st.texts[k].1.as_object_mut() {
                old.extend(new.iter().filter(|(k, _)| is_text_key(k)).map(|(k, x)| (k.clone(), x.clone())));
            } else {
                st.texts[k].1 = style.clone();
            }
        }
    });
    load_open(app);
}

/// Sets the fields from a preset. Missing or wrong keys keep the field as it is (older or newer presets).
pub fn apply_preset(app: &AppWindow, v: &serde_json::Value) {
    let int = |k: &str, max: i64, set: &dyn Fn(i32)| {
        if let Some(n) = v.get(k).and_then(|x| x.as_i64()).filter(|n| (0..=max).contains(n)) {
            set(n as i32)
        }
    };
    let boolean = |k: &str, set: &dyn Fn(bool)| v.get(k).and_then(|x| x.as_bool()).into_iter().for_each(set);
    let text = |k: &str, set: &dyn Fn(SharedString)| v.get(k).and_then(|x| x.as_str()).filter(|s| s.len() < 100).into_iter().for_each(|s| set(s.into()));
    boolean("px-units", &|b| app.set_px_units(b));
    int("paper", (layout::DIN.len() + layout::SOCIAL.len()) as i64, &|n| app.set_paper(n));
    text("custom-w", &|s| app.set_custom_w(s));
    text("custom-h", &|s| app.set_custom_h(s));
    boolean("landscape", &|b| app.set_landscape(b));
    text("dpi", &|s| app.set_dpi(s));
    text("margin", &|s| app.set_margin(s));
    text("gap", &|s| app.set_gap(s));
    boolean("auto-cols", &|b| app.set_auto_cols(b));
    text("cols", &|s| app.set_cols(s));
    // the format as typed ("2.39:1", "9:16") decides; the menu index only says Full frame or not
    int("picture", layout::PICTURE.len() as i64 + 1, &|n| app.set_picture(n));
    text("aspect", &|s| app.set_aspect(s));
    if app.get_picture() > 0 {
        picture_edited(app);
    }
    show_picture(app);
    boolean("trim", &|b| app.set_trim(b));
    text("bg-color", &|s| app.set_bg_color(s));
    boolean("line-on", &|b| app.set_line_on(b));
    text("line-width", &|s| app.set_line_width(s));
    text("line-inset", &|s| app.set_line_inset(s));
    text("line-color", &|s| app.set_line_color(s));
    int("line-pattern", layout::Pattern::ALL.len() as i64 - 1, &|n| app.set_line_pattern(n));
    text("global-angle", &|s| app.set_global_angle(s));
    boolean("mosaic-on", &|b| app.set_mosaic_on(b));
    text("mosaic-cols", &|s| app.set_mosaic_cols(s));
    text("mosaic-frames", &|s| app.set_mosaic_frames(s));
    text("mosaic-repeat", &|s| app.set_mosaic_repeat(s));
    text("mosaic-tint", &|s| app.set_mosaic_tint(s));
    text("mosaic-overlay", &|s| app.set_mosaic_overlay(s));
    apply_text_styles(app, v);
    int("export-format", 3, &|n| app.set_export_format(n));
    match_slots(app);
    // an A or social media size from the list fills the fields
    if app.get_paper() > 0 {
        show_paper(app);
    }
}
