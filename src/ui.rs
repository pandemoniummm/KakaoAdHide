//! The tray (Windows) / menu bar (macOS) app: first-time setup with consent, the local ad server, update checks.

use crate::feed::Feed;
use crate::notices::{self, Ask, Notice, Problem};
use crate::setup::{self, Status};
use crate::updater::{self, Check};
use crate::{platform, VERSION};
use std::time::Duration;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub fn message(title: &str, text: &str) {
    let _ = rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title(title).set_description(text)
        .set_buttons(rfd::MessageButtons::Ok).show();
}

pub fn confirm(title: &str, text: &str, ok: &str, cancel: &str) -> bool { confirm_label(title, text, ok, cancel) }

/// Something to know, nothing to decide: a notification on Windows (a message box if it can't be shown), a
/// message box elsewhere for now.
pub fn show(n: &Notice) {
    #[cfg(windows)]
    if platform::notify(&n.title, &n.body, &[], None) { return; }
    message(&n.title, &n.body);
}

/// A question; true when the user picks `ok`.
pub fn ask(a: &Ask) -> bool { confirm_label(&a.title, &a.body, a.ok, a.cancel) }

/// Something went wrong: a message box (not a notification that could be missed).
pub fn error(p: &Problem) { message(&p.title, &p.body) }

fn confirm_label(title: &str, text: &str, ok: &str, cancel: &str) -> bool {
    match rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title(title).set_description(text)
        .set_buttons(rfd::MessageButtons::OkCancelCustom(ok.into(), cancel.into())).show()
    {
        rfd::MessageDialogResult::Ok => true,
        rfd::MessageDialogResult::Custom(label) => label == ok,
        _ => false,
    }
}

enum UserEvent {
    Menu(MenuId),
    UpdateChecked(Result<Check, String>, bool),
    TimerCheck,
    RouteCheck,
    #[cfg_attr(not(windows), allow(dead_code))]
    Quit,
    /// The blank ad's logo was clicked: show the menu at the cursor.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    ShowMenu,
    /// The "new version" notification or its Update button was clicked: install (Windows).
    #[cfg_attr(not(windows), allow(dead_code))]
    InstallUpdate,
}

#[derive(Clone, Copy)]
enum Action { Setup, Check, Update, Changes, Skip, AutoCheck, Login, Uninstall, Quit }

struct Items {
    _status: MenuItem,
    setup: MenuItem,
    update: MenuItem,
    changes: MenuItem,
    skip: MenuItem,
    check: MenuItem,
    auto_check: CheckMenuItem,
    login: CheckMenuItem,
    uninstall: MenuItem,
    quit: MenuItem,
}

struct App {
    status: Status,
    pending: Option<Feed>,
    tray: Option<TrayIcon>,
    items: Option<Items>,
    proxy: EventLoopProxy<UserEvent>,
    /// A hidden window to own the menu opened from the blank ad.
    #[cfg(any(windows, target_os = "macos"))]
    menu_window: Option<tao::window::Window>,
}

impl App {
    fn status_text(&self) -> String {
        match &self.status {
            Status::Serving => notices::status::SERVING.into(),
            Status::NotSetUp => notices::status::NOT_SET_UP.into(),
            Status::PortBusy(_) => notices::status::PORT_BUSY.into(),
            Status::NotRouted => notices::status::NOT_ROUTED.into(),
            Status::Failed(e) => notices::status::failed(e),
        }
    }

    fn build_menu(&mut self) -> Menu {
        let menu = Menu::new();
        let header = MenuItem::new(format!("KakaoAdHide {VERSION}"), false, None);
        let status = MenuItem::new(self.status_text(), false, None);
        use notices::menu as m;
        let setup = MenuItem::new(m::SETUP, self.status != Status::Serving, None);
        let update = MenuItem::new(m::UPDATE, false, None);
        let changes = MenuItem::new(m::CHANGES, false, None);
        let skip = MenuItem::new(m::SKIP, false, None);
        let check = MenuItem::new(m::CHECK, true, None);
        let auto_check = CheckMenuItem::new(m::AUTO_CHECK, true, updater::enabled(), None);
        let login = CheckMenuItem::new(m::LOGIN, cfg!(target_os = "macos"), platform::login_item_enabled(), None);
        let uninstall = MenuItem::new(m::UNINSTALL, true, None);
        let quit = MenuItem::new(m::QUIT, true, None);
        let _ = menu.append_items(&[&header, &status]);
        if self.status != Status::Serving { let _ = menu.append(&setup); }
        let _ = menu.append(&PredefinedMenuItem::separator());
        if let Some(f) = &self.pending {
            update.set_text(m::update_to(&f.version));
            update.set_enabled(true);
            changes.set_enabled(true);
            skip.set_enabled(true);
            let _ = menu.append_items(&[&update, &changes, &skip]);
        } else {
            let _ = menu.append(&check);
        }
        let _ = menu.append(&auto_check);
        if cfg!(target_os = "macos") { let _ = menu.append(&login); }
        let _ = menu.append_items(&[&PredefinedMenuItem::separator(), &uninstall, &quit]);
        self.items = Some(Items { _status: status, setup, update, changes, skip, check, auto_check, login, uninstall, quit });
        menu
    }

    fn refresh(&mut self) {
        let menu = self.build_menu();
        if let Some(t) = &self.tray {
            t.set_menu(Some(Box::new(menu)));
            let i = icon(self.pending.is_some(), self.status != Status::Serving);
            #[cfg(target_os = "macos")]
            let _ = t.set_icon_templated(Some(i));
            #[cfg(not(target_os = "macos"))]
            let _ = t.set_icon(Some(i));
            let _ = t.set_tooltip(Some(format!("KakaoAdHide {VERSION} - {}", self.status_text())));
        }
    }

    /// Setup (with consent the first time), then the server.
    fn start(&mut self, ask: bool) {
        let m = setup::missing();
        if m.any() {
            let agreed = platform::get("Consented").is_some();
            if !agreed && !ask { self.status = Status::NotSetUp; return; }
            if !agreed && !self::ask(&notices::first_setup()) {
                self.status = Status::NotSetUp;
                return;
            }
            platform::set("Consented", Some("1"));
            if let Err(e) = setup::complete(&m) {
                self.status = Status::NotSetUp;
                error(&notices::setup_failed(&e));
                return;
            }
        }
        self.status = setup::serve();
        if let Status::PortBusy(e) = &self.status {
            error(&notices::port_busy(e));
        }
    }

    fn check_updates(&self, interactive: bool) {
        let proxy = self.proxy.clone();
        std::thread::spawn(move || { let _ = proxy.send_event(UserEvent::UpdateChecked(updater::check(), interactive)); });
    }

    fn install(&mut self, f: Feed) -> bool {
        if !platform::can_update_in_place() {
            if ask(&notices::update_not_possible()) { platform::open_url(&f.page_url()); }
            return false;
        }
        match platform::install_update(&f) {
            Ok(()) => true,
            Err(updater::InstallError::Verify(e)) => {
                crate::server::note(&format!("update {} not verified: {e}", f.version));
                if ask(&notices::update_unverified()) { platform::open_url(&f.page_url()); }
                false
            }
            Err(e) => { error(&notices::download_failed(&e.to_string())); false }
        }
    }

    fn offer(&mut self, f: Feed) -> bool {
        if ask(&notices::offer_update(&f.version)) { return self.install(f); }
        false
    }
}

// ---------------------------------------------------------------- event handlers (true: exit)

impl App {
    /// First event: setup and server, KakaoTalk (Windows, started by hand), the tray icon, the blank ad's menu.
    fn init(&mut self, at_sign_in: bool, target: &EventLoopWindowTarget<UserEvent>) {
        platform::first_run();
        #[cfg(windows)]
        platform::register_notifications();
        let updated_from = updater::note_version();
        self.start(true);
        #[cfg(windows)]
        platform::start_kakao(at_sign_in);
        #[cfg(not(windows))]
        let _ = at_sign_in;
        #[cfg(not(any(windows, target_os = "macos")))]
        let _ = target;
        let menu = self.build_menu();
        let i = icon(false, self.status != Status::Serving);
        let builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(format!("KakaoAdHide {VERSION}"));
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(i);
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(i);
        self.tray = builder.build().ok();
        self.refresh();
        if let Some(prev) = &updated_from {
            show(&notices::updated(prev));
        }
        #[cfg(any(windows, target_os = "macos"))]
        {
            // the menu opened from the blank ad's button
            self.menu_window = tao::window::WindowBuilder::new().with_visible(false).with_title("KakaoAdHide").build(target).ok();
            if self.menu_window.is_some() {
                let p = self.proxy.clone();
                crate::server::set_menu_handler(menu_token(), move || { let _ = p.send_event(UserEvent::ShowMenu); });
            }
            platform::watch_kakao_active();
        }
    }

    /// The blank ad's button was clicked: the tray's menu at the cursor.
    #[cfg(windows)]
    fn show_menu(&mut self) {
        use tao::platform::windows::WindowExtWindows;
        use tray_icon::menu::ContextMenu;
        let Some(hwnd) = self.menu_window.as_ref().map(WindowExtWindows::hwnd) else { return };
        // the same menu as the tray's (one menu, so its items match what the event handler expects)
        let menu = self.build_menu();
        if let Some(t) = &self.tray { t.set_menu(Some(Box::new(menu.clone()))); }
        // KakaoTalk dims its icons while the menu is open: the button too (not left to the foreground event, which
        // a hidden window doesn't reliably get), then whatever is in front once it closes
        crate::server::set_kakao_active(false);
        let prev = platform::take_foreground(hwnd);
        unsafe { menu.show_context_menu_for_hwnd(hwnd, None); }
        platform::give_back_foreground(hwnd, prev);
    }

    /// The blank ad's button was clicked: the menu bar menu at the cursor.
    #[cfg(target_os = "macos")]
    fn show_menu(&mut self) {
        use tao::platform::macos::WindowExtMacOS;
        use tray_icon::menu::ContextMenu;
        let Some(view) = self.menu_window.as_ref().map(WindowExtMacOS::ns_view) else { return };
        let menu = self.build_menu();
        if let Some(t) = &self.tray { t.set_menu(Some(Box::new(menu.clone()))); }
        // SAFETY: the view belongs to menu_window, which lives as long as the app; None = at the mouse pointer
        unsafe { menu.show_context_menu_for_nsview(view as *const _, None); }
    }

    fn route_check(&mut self) {
        let next = match &self.status {
            Status::Serving if !setup::routed() => Some(Status::NotRouted),
            Status::NotRouted if setup::routed() => Some(Status::Serving),
            _ => None,
        };
        if let Some(n) = next { self.status = n; self.refresh(); }
    }

    fn update_checked(&mut self, result: Result<Check, String>, interactive: bool) -> bool {
        match result {
            Ok(Check::Available(f)) => {
                let skipped = updater::skipped().as_deref() == Some(f.version.as_str());
                if !interactive && skipped { return false; }
                self.pending = Some(f.clone());
                self.refresh();
                let first_notice = updater::notified().as_deref() != Some(f.version.as_str());
                if interactive || first_notice {
                    updater::set_notified(&f.version);
                    // Windows: a notification with Update / Later (found by the daily check or from the menu);
                    // the notification itself or Update installs, no further question
                    #[cfg(windows)]
                    {
                        let p = self.proxy.clone();
                        let n = notices::new_version(&f.version);
                        let shown = platform::notify(&n.title, &n.body, &[(notices::UPDATE, "update"), (notices::LATER, "later")],
                            Some(Box::new(move |id| {
                                if id.as_deref() != Some("later") { let _ = p.send_event(UserEvent::InstallUpdate); }
                            })));
                        if shown { return false; }
                    }
                    return self.offer(f);
                }
            }
            Ok(Check::UpToDate) => {
                self.pending = None;
                self.refresh();
                if interactive { show(&notices::up_to_date()); }
            }
            Err(e) => if interactive { error(&notices::check_failed(&e)) },
        }
        false
    }

    fn action(&self, id: &MenuId) -> Option<Action> {
        let items = self.items.as_ref()?;
        [
            (items.setup.id(), Action::Setup), (items.check.id(), Action::Check), (items.update.id(), Action::Update),
            (items.changes.id(), Action::Changes), (items.skip.id(), Action::Skip), (items.auto_check.id(), Action::AutoCheck),
            (items.login.id(), Action::Login), (items.uninstall.id(), Action::Uninstall), (items.quit.id(), Action::Quit),
        ].into_iter().find(|(i, _)| *i == id).map(|(_, a)| a)
    }

    fn menu(&mut self, id: &MenuId) -> bool {
        match self.action(id) {
            Some(Action::Setup) => {
                if self.status == Status::NotRouted {
                    if let Err(e) = platform::set_hosts(true) { error(&notices::setup_failed(&e)); }
                    self.status = if setup::routed() { Status::Serving } else { Status::NotRouted };
                } else {
                    platform::set("Consented", None);
                    self.start(true);
                }
                self.refresh();
            }
            Some(Action::Check) => self.check_updates(true),
            Some(Action::Update) => if let Some(f) = self.pending.clone() { return self.install(f) },
            Some(Action::Changes) => if let Some(f) = &self.pending { platform::open_url(&f.page_url()) },
            Some(Action::Skip) => {
                if let Some(f) = self.pending.take() { updater::skip(&f.version); }
                self.refresh();
            }
            Some(Action::AutoCheck) => { updater::set_enabled(!updater::enabled()); self.refresh(); }
            Some(Action::Login) => {
                if let Err(e) = platform::set_login_item(!platform::login_item_enabled()) { error(&notices::login_item_failed(&e)); }
                self.refresh();
            }
            Some(Action::Uninstall) => {
                if !ask(&notices::uninstall()) { return false; }
                // Windows says how it went itself (the same uninstall runs from Settings > Apps)
                let result = platform::uninstall(false);
                if cfg!(target_os = "macos") {
                    match result {
                        Ok(()) => show(&notices::removed()),
                        Err(e) => error(&notices::uninstall_incomplete(&e)),
                    }
                }
                return true;
            }
            Some(Action::Quit) => return true,
            None => {}
        }
        false
    }
}

/// Background timers: update checks a minute after start and then daily; every 10 minutes whether the ad hosts are
/// still sent here (hosts file edited, resolver cache).
fn start_timers(proxy: &EventLoopProxy<UserEvent>) {
    let p = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        loop {
            let _ = p.send_event(UserEvent::TimerCheck);
            std::thread::sleep(Duration::from_secs(24 * 60 * 60));
        }
    });
    let p = proxy.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(10 * 60));
        let _ = p.send_event(UserEvent::RouteCheck);
    });
}

pub fn run(at_sign_in: bool) -> ! {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    let mut event_loop = event_loop;
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();
    {
        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| { let _ = p.send_event(UserEvent::Menu(e.id)); }));
    }
    #[cfg(windows)]
    {
        let p = proxy.clone();
        platform::watch_quit_event(move || { let _ = p.send_event(UserEvent::Quit); std::thread::sleep(Duration::from_secs(3)); std::process::exit(0); });
    }
    start_timers(&proxy);

    let mut app = App {
        status: Status::NotSetUp, pending: None, tray: None, items: None, proxy,
        #[cfg(any(windows, target_os = "macos"))]
        menu_window: None,
    };

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        let exit = match event {
            Event::NewEvents(StartCause::Init) => { app.init(at_sign_in, target); false }
            #[cfg(any(windows, target_os = "macos"))]
            Event::UserEvent(UserEvent::ShowMenu) => { app.show_menu(); false }
            #[cfg(windows)]
            Event::UserEvent(UserEvent::InstallUpdate) => app.pending.clone().is_some_and(|f| app.install(f)),
            Event::UserEvent(UserEvent::Quit) => true,
            Event::UserEvent(UserEvent::RouteCheck) => { app.route_check(); false }
            Event::UserEvent(UserEvent::TimerCheck) => { if updater::enabled() { app.check_updates(false); } false }
            Event::UserEvent(UserEvent::UpdateChecked(result, interactive)) => app.update_checked(result, interactive),
            Event::UserEvent(UserEvent::Menu(id)) => app.menu(&id),
            _ => false,
        };
        if exit { *control_flow = ControlFlow::Exit; }
    })
}

/// The token for the blank ad's menu button (see server::MENU_PATH). Made once and kept in the settings, so the
/// blank ad KakaoTalk already holds (it asks again only every hour) keeps working after KakaoAdHide restarts or
/// updates.
#[cfg(any(windows, target_os = "macos"))]
fn menu_token() -> String {
    // A token kept from 2.1.0-2.2.0 (hash based) stays: replacing it would break the button in the blank ad
    // KakaoTalk already holds until its next request (up to a day). It only guards popping the menu up.
    if let Some(t) = platform::get("MenuToken").filter(|t| t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit())) {
        return t;
    }
    let t = random_token();
    platform::set("MenuToken", Some(&t));
    t
}

/// 32 hex characters from the system's secure random source.
#[cfg(any(windows, target_os = "macos"))]
fn random_token() -> String {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut b).is_err() {
        // practically never; still unguessable enough for what the token guards (popping the menu up)
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        b = (nanos ^ (u128::from(std::process::id()) << 64)).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The tray (Windows) / menu bar (macOS) icon, drawn here (no image files): the app icon's shapes, a chat bubble
/// above and the emptied ad slot dashed below (assets/icon-line.svg). Windows: the bubble in the app's mint, the
/// slot in a grey that shows on light and dark taskbars; macOS: a template (black, tinted by the system). An
/// outlined, grey bubble while not working; a dot when an update is waiting.
fn icon(update: bool, inactive: bool) -> Icon {
    Icon::from_rgba(icon_rgba(update, inactive), ICON_SIZE as u32, ICON_SIZE as u32).expect("icon")
}

const ICON_SIZE: usize = 32;

fn icon_rgba(update: bool, inactive: bool) -> Vec<u8> {
    const S: usize = ICON_SIZE;
    const GREY: [u8; 3] = [0x8a, 0x93, 0xa6];
    let (bubble, slot, dot) = if cfg!(target_os = "macos") {
        ([0; 3], [0; 3], [0; 3])
    } else {
        (if inactive { GREY } else { [0x2f, 0xb8, 0xc6] }, GREY, [0xf2, 0x55, 0x7a])
    };
    // signed distance to a rounded rectangle (centre cx,cy, half size hw,hh, corner radius r)
    let sdf = |x: f32, y: f32, cy: f32| {
        let (hw, hh, r) = (12.0f32, 4.5f32, 4.5f32);
        let (qx, qy) = ((x - 16.0).abs() - hw + r, (y - cy).abs() - hh + r);
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
    };
    let mut px = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            // coverage of each layer, 4x4 supersampled for smooth edges
            let mut cover = [0f32; 3];
            for sy in 0..4 {
                for sx in 0..4 {
                    let (fx, fy) = (x as f32 + (sx as f32 + 0.5) / 4.0, y as f32 + (sy as f32 + 0.5) / 4.0);
                    // the bubble (y 5..14): filled, an outline while not working
                    let d = sdf(fx, fy, 9.5);
                    if if inactive { (-2.6..=0.0).contains(&d) } else { d <= 0.0 } { cover[0] += 1.0 / 16.0; }
                    // the slot (y 18..27): an outline drawn inside the edge, so it is as big as the bubble
                    let d = sdf(fx, fy, 22.5);
                    if (-2.6..=0.0).contains(&d) { cover[1] += 1.0 / 16.0; }
                    // the update dot, top right
                    if update && (fx - 27.0).hypot(fy - 5.0) <= 4.5 { cover[2] += 1.0 / 16.0; }
                }
            }
            // later layers on top: bubble, slot, dot
            let i = (y * S + x) * 4;
            for (a, c) in [(cover[0], bubble), (cover[1], slot), (cover[2], dot)] {
                if a > 0.0 {
                    px[i..i + 3].copy_from_slice(&c);
                    px[i + 3] = px[i + 3].max((a.min(1.0) * 255.0) as u8);
                }
            }
        }
    }
    px
}

#[cfg(test)]
mod tests {
    /// Writes the tray icon states to target/tray-*.rgba (32x32 RGBA) to look at: `cargo test tray_preview -- --ignored`
    #[test]
    #[ignore]
    fn tray_preview() {
        for (name, update, inactive) in [("on", false, false), ("off", false, true), ("update", true, false)] {
            std::fs::write(format!("target/tray-{name}.rgba"), super::icon_rgba(update, inactive)).unwrap();
        }
    }
}
