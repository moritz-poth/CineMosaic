//! The parts of the app that talk to macOS directly (AppKit through objc2): the colour panel, the app menu
//! (About, Settings…), the app delegate (open from the Finder, Quit with unsaved work) and the windows (minimum
//! size, sRGB, appearance, title bar).

/// The macOS colour panel: opened with a colour, it reports every colour picked (polled; it has no Slint target)
/// until it is closed or opened for another preset.
pub mod color_panel {
    use std::cell::RefCell;
    use std::rc::Rc;

    type OnPick = Rc<dyn Fn([u8; 3])>;
    type OnClose = Rc<dyn Fn()>;

    thread_local! {
        static TARGET: RefCell<Option<(OnPick, OnClose, [u8; 3])>> = const { RefCell::new(None) };
        static TIMER: slint::Timer = slint::Timer::default();
    }

    #[cfg(target_os = "macos")]
    pub fn open(c: [u8; 3], on_pick: OnPick, on_close: OnClose) {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        unsafe {
            let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
            let _: () = msg_send![panel, setShowsAlpha: false];
            let [r, g, b] = c.map(|v| v as f64 / 255.0);
            let color: *mut AnyObject = msg_send![class!(NSColor), colorWithSRGBRed: r, green: g, blue: b, alpha: 1.0f64];
            let _: () = msg_send![panel, setColor: color];
            let _: () = msg_send![panel, orderFront: std::ptr::null_mut::<AnyObject>()];
        }
        // a panel still open for another colour row: that one is done
        if let Some((_, close, _)) = TARGET.with(|t| t.borrow_mut().replace((on_pick, on_close, c))) {
            close();
        }
        TIMER.with(|t| t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(80), poll));
    }

    #[cfg(not(target_os = "macos"))]
    pub fn open(_: [u8; 3], _: OnPick, _: OnClose) {}

    #[cfg(target_os = "macos")]
    fn poll() {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        let picked = unsafe {
            let panel: *mut AnyObject = msg_send![class!(NSColorPanel), sharedColorPanel];
            let visible: bool = msg_send![panel, isVisible];
            if !visible {
                TIMER.with(|t| t.stop());
                if let Some((_, close, _)) = TARGET.with(|t| t.borrow_mut().take()) {
                    close();
                }
                return;
            }
            let color: *mut AnyObject = msg_send![panel, color];
            let space: *mut AnyObject = msg_send![class!(NSColorSpace), sRGBColorSpace];
            let srgb: *mut AnyObject = msg_send![color, colorUsingColorSpace: space];
            if srgb.is_null() {
                return;
            }
            let (r, g, b): (f64, f64, f64) = (msg_send![srgb, redComponent], msg_send![srgb, greenComponent], msg_send![srgb, blueComponent]);
            [r, g, b].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        };
        let call = TARGET.with(|t| {
            let mut t = t.borrow_mut();
            let (f, _, last) = t.as_mut()?;
            (*last != picked).then(|| {
                *last = picked;
                f.clone()
            })
        });
        if let Some(f) = call {
            f(picked);
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn poll() {}
}

#[cfg(target_os = "macos")]
pub mod mac_menu {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{class, msg_send, sel};
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    type Handler = RefCell<Option<Box<dyn Fn()>>>;
    thread_local! {
        static ABOUT: Handler = RefCell::new(None);
        static SETTINGS: Handler = RefCell::new(None);
        static TITLE: RefCell<String> = RefCell::new("Settings…".into());
    }
    static ORIGINAL_SET_MAIN_MENU: AtomicUsize = AtomicUsize::new(0);
    type ObjFn = extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);

    extern "C-unwind" fn about(_: *mut AnyObject, _: Sel, _: *mut AnyObject) {
        ABOUT.with(|h| h.borrow().iter().for_each(|f| f()));
    }

    extern "C-unwind" fn settings(_: *mut AnyObject, _: Sel, _: *mut AnyObject) {
        SETTINGS.with(|h| h.borrow().iter().for_each(|f| f()));
    }

    extern "C-unwind" fn set_main_menu(app: *mut AnyObject, sel: Sel, menu: *mut AnyObject) {
        let original = ORIGINAL_SET_MAIN_MENU.load(Ordering::Relaxed);
        if original != 0 {
            let original: ObjFn = unsafe { std::mem::transmute(original) };
            original(app, sel, menu);
        }
        ensure();
    }

    fn ns_string(s: &str) -> Retained<AnyObject> {
        let c = std::ffi::CString::new(s).unwrap_or_default();
        unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
    }

    /// Adds "Settings…" to the app menu if it is missing and keeps its title in the current language.
    pub fn ensure() {
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let main: *mut AnyObject = msg_send![app, mainMenu];
            if main.is_null() {
                return;
            }
            let count: isize = msg_send![main, numberOfItems];
            if count == 0 {
                return;
            }
            let first: *mut AnyObject = msg_send![main, itemAtIndex: 0isize];
            let app_menu: *mut AnyObject = msg_send![first, submenu];
            if app_menu.is_null() {
                return;
            }
            let title = TITLE.with(|t| ns_string(&t.borrow()));
            let index: isize = msg_send![app_menu, indexOfItemWithTarget: std::ptr::null_mut::<AnyObject>(), andAction: sel!(ssShowSettings:)];
            if index >= 0 {
                let item: *mut AnyObject = msg_send![app_menu, itemAtIndex: index];
                let _: () = msg_send![item, setTitle: &*title];
                return;
            }
            // only into the app menu Slint builds (About, separator, …); macOS sets its own short menu at launch,
            // and inserting past its end throws an Objective-C exception that aborts the app
            let items: isize = msg_send![app_menu, numberOfItems];
            if items < 3 {
                return;
            }
            let second: *mut AnyObject = msg_send![app_menu, itemAtIndex: 1isize];
            let separator_at_1: bool = msg_send![second, isSeparatorItem];
            if !separator_at_1 {
                return;
            }
            let item: Retained<AnyObject> = msg_send![class!(NSMenuItem), new];
            let _: () = msg_send![&*item, setTitle: &*title];
            let _: () = msg_send![&*item, setAction: sel!(ssShowSettings:)];
            let _: () = msg_send![&*item, setKeyEquivalent: &*ns_string(",")];
            let separator: Retained<AnyObject> = msg_send![class!(NSMenuItem), separatorItem];
            // About, separator, here: Settings…, separator, Services …
            let _: () = msg_send![app_menu, insertItem: &*item, atIndex: 2isize];
            let _: () = msg_send![app_menu, insertItem: &*separator, atIndex: 3isize];
        }
    }

    pub fn set_settings_title(title: &str) {
        TITLE.with(|t| *t.borrow_mut() = title.to_string());
        ensure();
    }

    pub fn install(on_about: Box<dyn Fn()>, on_settings: Box<dyn Fn()>, settings_title: &str) {
        ABOUT.with(|h| *h.borrow_mut() = Some(on_about));
        SETTINGS.with(|h| *h.borrow_mut() = Some(on_settings));
        TITLE.with(|t| *t.borrow_mut() = settings_title.to_string());
        let Some(cls) = AnyClass::get(c"NSApplication") else { return };
        let imp = |f: ObjFn| unsafe { std::mem::transmute::<ObjFn, Imp>(f) };
        for sel in [sel!(orderFrontStandardAboutPanel:), sel!(orderFrontStandardAboutPanelWithOptions:)] {
            if let Some(m) = cls.instance_method(sel) {
                // same signature (self, _cmd, one object) as the methods it replaces
                unsafe { m.set_implementation(imp(about)) };
            }
        }
        // the menu item's action; with no target it reaches NSApplication through the responder chain
        unsafe { objc2::ffi::class_addMethod(cls as *const AnyClass as *mut AnyClass, sel!(ssShowSettings:), imp(settings), c"v@:@".as_ptr()) };
        if let Some(m) = cls.instance_method(sel!(setMainMenu:)) {
            let original = unsafe { m.set_implementation(imp(set_main_menu)) };
            ORIGINAL_SET_MAIN_MENU.store(original as usize, Ordering::Relaxed);
        }
        ensure();
    }
}

/// Project files from the Finder (double-click, drop on the Dock icon) and Quit asking about unsaved work: two
/// methods added to winit's application delegate class (winit registers it but offers neither).
#[cfg(target_os = "macos")]
pub mod mac_app {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::sel;
    use std::cell::RefCell;
    use std::path::PathBuf;

    type Open = RefCell<Option<Box<dyn Fn(PathBuf)>>>;
    type Quit = RefCell<Option<Box<dyn Fn() -> bool>>>;
    thread_local! {
        static OPEN: Open = RefCell::new(None);
        static QUIT: Quit = RefCell::new(None);
    }

    extern "C-unwind" fn open_urls(_: *mut AnyObject, _: Sel, _: *mut AnyObject, urls: *mut AnyObject) {
        let count: usize = unsafe { msg_send![urls, count] };
        for i in 0..count {
            let path: *mut AnyObject = unsafe {
                let url: *mut AnyObject = msg_send![urls, objectAtIndex: i];
                msg_send![url, path]
            };
            if path.is_null() {
                continue;
            }
            let c: *const std::ffi::c_char = unsafe { msg_send![path, UTF8String] };
            if c.is_null() {
                continue;
            }
            let p = PathBuf::from(unsafe { std::ffi::CStr::from_ptr(c) }.to_string_lossy().into_owned());
            OPEN.with(|h| h.borrow().iter().for_each(|f| f(p.clone())));
        }
    }

    /// NSTerminateNow (1) or NSTerminateCancel (0)
    extern "C-unwind" fn should_terminate(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> usize {
        QUIT.with(|h| h.borrow().as_ref().is_none_or(|f| f())) as usize
    }

    pub fn install(open: Box<dyn Fn(PathBuf)>, quit: Box<dyn Fn() -> bool>) {
        OPEN.with(|h| *h.borrow_mut() = Some(open));
        QUIT.with(|h| *h.borrow_mut() = Some(quit));
        let Some(cls) = AnyClass::get(c"WinitApplicationDelegate") else {
            crate::log::write("no WinitApplicationDelegate: projects cannot be opened from the Finder");
            return;
        };
        type OpenFn = extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject);
        type QuitFn = extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize;
        let cls = cls as *const AnyClass as *mut AnyClass;
        unsafe {
            objc2::ffi::class_addMethod(cls, sel!(application:openURLs:), std::mem::transmute::<OpenFn, Imp>(open_urls), c"v@:@@".as_ptr());
            objc2::ffi::class_addMethod(cls, sel!(applicationShouldTerminate:), std::mem::transmute::<QuitFn, Imp>(should_terminate), c"Q@:@".as_ptr());
        }
    }
}

/// AppKit details Slint does not offer: colours in sRGB like the Swift apps, and the settings window's title bar.
#[cfg(target_os = "macos")]
pub mod mac_window {
    use objc2::encode::{Encode, Encoding};
    use objc2::runtime::{AnyObject, Imp, Sel};
    use objc2::{class, msg_send, sel};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Size {
        width: f64,
        height: f64,
    }
    unsafe impl Encode for Size {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    unsafe impl Encode for Rect {
        const ENCODING: Encoding = Encoding::Struct("CGRect", &[Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]), Size::ENCODING]);
    }

    type Init = extern "C-unwind" fn(*mut AnyObject, Sel, Rect, usize, usize, bool) -> *mut AnyObject;
    static ORIGINAL_INIT: AtomicUsize = AtomicUsize::new(0);

    extern "C-unwind" fn init_srgb(this: *mut AnyObject, sel: Sel, rect: Rect, style: usize, backing: usize, defer: bool) -> *mut AnyObject {
        let original: Init = unsafe { std::mem::transmute(ORIGINAL_INIT.load(Ordering::Relaxed)) };
        let win = original(this, sel, rect, style, backing, defer);
        if !win.is_null() {
            unsafe {
                let srgb: *mut AnyObject = msg_send![class!(NSColorSpace), sRGBColorSpace];
                let _: () = msg_send![win, setColorSpace: srgb];
            }
        }
        win
    }

    /// Slint (femtovg, OpenGL) draws without colour management, so #1f2225 came out as #21262a next to Mountpoint's
    /// exact #1f2225. A window tagged as sRGB before its first frame shows the Theme colours exactly like the SwiftUI
    /// apps; set later it only worked now and then – so every NSWindow gets sRGB right in its initializer.
    /// Call once, before the first window is created.
    pub fn srgb_windows() {
        let cls = class!(NSWindow);
        if let Some(m) = cls.instance_method(sel!(initWithContentRect:styleMask:backing:defer:)) {
            let imp: Imp = unsafe { std::mem::transmute(init_srgb as Init) };
            let original = unsafe { m.set_implementation(imp) };
            ORIGINAL_INIT.store(original as usize, Ordering::Relaxed);
        }
    }

    type KeyFn = extern "C-unwind" fn(*mut AnyObject, Sel);
    static ORIGINAL_BECOME_KEY: AtomicUsize = AtomicUsize::new(0);
    static ORIGINAL_RESIGN_KEY: AtomicUsize = AtomicUsize::new(0);
    thread_local! {
        static KEY_CHANGED: std::cell::RefCell<Option<Box<dyn Fn()>>> = std::cell::RefCell::new(None);
    }

    fn key_changed(original: &AtomicUsize, this: *mut AnyObject, sel: Sel) {
        let original: KeyFn = unsafe { std::mem::transmute(original.load(Ordering::Relaxed)) };
        original(this, sel);
        KEY_CHANGED.with(|f| f.borrow().iter().for_each(|f| f()));
    }
    extern "C-unwind" fn become_key(this: *mut AnyObject, sel: Sel) {
        key_changed(&ORIGINAL_BECOME_KEY, this, sel);
    }
    extern "C-unwind" fn resign_key(this: *mut AnyObject, sel: Sel) {
        key_changed(&ORIGINAL_RESIGN_KEY, this, sel);
    }

    /// Calls `f` whenever any window becomes or stops being the key window – the tab bar then changes colour at once
    /// together with the title bar (polling made it lag behind).
    pub fn on_key_change(f: Box<dyn Fn()>) {
        KEY_CHANGED.with(|k| *k.borrow_mut() = Some(f));
        let cls = class!(NSWindow);
        for (name, imp, original) in [
            (sel!(becomeKeyWindow), become_key as KeyFn, &ORIGINAL_BECOME_KEY),
            (sel!(resignKeyWindow), resign_key as KeyFn, &ORIGINAL_RESIGN_KEY),
        ] {
            if original.load(Ordering::Relaxed) != 0 {
                continue;
            }
            if let Some(m) = cls.instance_method(name) {
                let imp: Imp = unsafe { std::mem::transmute(imp) };
                original.store(unsafe { m.set_implementation(imp) } as usize, Ordering::Relaxed);
            }
        }
    }

    fn ns_window(w: &slint::Window) -> Option<*mut AnyObject> {
        let handle = w.window_handle();
        let RawWindowHandle::AppKit(h) = handle.window_handle().ok()?.as_raw() else { return None };
        let view = h.ns_view.as_ptr() as *mut AnyObject;
        let win: *mut AnyObject = unsafe { msg_send![view, window] };
        (!win.is_null()).then_some(win)
    }

    /// Content up to the top edge under a transparent title bar (like an NSToolbar window), so the tab bar and the
    /// title bar are one surface without the separator line. Keeps the content size (`height` = whole window).
    pub fn unify_titlebar(w: &slint::Window, width: f64, height: f64) {
        let Some(win) = ns_window(w) else { return };
        unsafe {
            let mask: usize = msg_send![win, styleMask];
            const FULL_SIZE_CONTENT_VIEW: usize = 1 << 15;
            const MINIATURIZABLE: usize = 1 << 2;
            if mask & FULL_SIZE_CONTENT_VIEW == 0 {
                // HIG settings: minimize and zoom dimmed (zoom already is: the size is fixed)
                let _: () = msg_send![win, setStyleMask: (mask | FULL_SIZE_CONTENT_VIEW) & !MINIATURIZABLE];
                let _: () = msg_send![win, setTitlebarAppearsTransparent: true];
                let _: () = msg_send![win, setContentSize: Size { width, height }];
            }
        }
    }

    /// Usable size of the main screen (without menu bar and Dock), in points; (0, 0) if unknown.
    pub fn screen_size() -> (f64, f64) {
        unsafe {
            let screen: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
            if screen.is_null() {
                return (0.0, 0.0);
            }
            let frame: Rect = msg_send![screen, visibleFrame];
            (frame.width, frame.height)
        }
    }

    /// Smallest size the user can drag the window to (content size in points); returns the value AppKit now holds.
    pub fn set_min_size(w: &slint::Window, width: f64, height: f64) -> Option<(f64, f64)> {
        let win = ns_window(w)?;
        unsafe {
            let _: () = msg_send![win, setContentMinSize: Size { width, height }];
            let got: Size = msg_send![win, contentMinSize];
            Some((got.width, got.height))
        }
    }

    /// Title bar, traffic lights and title text in the app's own light/dark (not the system's): every window.
    pub fn appearance_all(dark: bool) {
        let name: &std::ffi::CStr = if dark { c"NSAppearanceNameDarkAqua" } else { c"NSAppearanceNameAqua" };
        unsafe {
            let name: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
            let wanted: *mut AnyObject = msg_send![class!(NSAppearance), appearanceNamed: name];
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let windows: *mut AnyObject = msg_send![app, windows];
            let count: usize = msg_send![windows, count];
            for i in 0..count {
                let w: *mut AnyObject = msg_send![windows, objectAtIndex: i];
                let current: *mut AnyObject = msg_send![w, appearance];
                if current != wanted {
                    let _: () = msg_send![w, setAppearance: wanted];
                }
            }
        }
    }

    /// Whether the window is the key window (the title bar is lighter then; the tab bar follows).
    pub fn is_key(w: &slint::Window) -> bool {
        ns_window(w).is_some_and(|win| unsafe { msg_send![win, isKeyWindow] })
    }
}
