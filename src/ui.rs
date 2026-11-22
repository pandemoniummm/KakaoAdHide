//! The tray (Windows) / menu bar (macOS) app: first-time setup with consent, the local ad server, update checks.

use crate::feed::Feed;
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

fn confirm_label(title: &str, text: &str, ok: &str, cancel: &str) -> bool {
    match rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title(title).set_description(text)
        .set_buttons(rfd::MessageButtons::OkCancelCustom(ok.into(), cancel.into())).show()
    {
        rfd::MessageDialogResult::Ok => true,
        rfd::MessageDialogResult::Custom(label) => label == ok,
        _ => false,
    }
}

const CONSENT: &str = "카카오톡 광고 칸을 비우려면 처음 한 번 설정이 필요합니다.\n\n\
• 이 컴퓨터에서만 쓰는 인증서를 만들고 신뢰하도록 등록합니다. 카카오톡 광고 주소 두 개(serv.ds.kakao.com, display.ad.daum.net)에만 쓸 수 있고, 다른 사이트에는 쓸 수 없습니다.\n\
• hosts 파일에 두 광고 주소를 이 컴퓨터로 보내는 줄을 넣습니다(관리자 권한 필요).\n\
• 그러면 KakaoAdHide가 광고 대신 빈 칸을 돌려줍니다. 광고 서버에는 접속하지 않습니다.\n\n\
같은 광고 주소를 쓰는 웹사이트(다음 등)의 광고도 함께 안 보일 수 있습니다. 제거하면 모두 원래대로 돌아갑니다.";

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
            Status::Serving => "광고 칸을 비우는 중".into(),
            Status::NotSetUp => "설정이 필요합니다".into(),
            Status::PortBusy(_) => "443 포트를 다른 프로그램이 쓰고 있습니다".into(),
            Status::NotRouted => "광고 주소가 이 컴퓨터로 오지 않습니다".into(),
            Status::Failed(e) => format!("오류: {e}"),
        }
    }

    fn build_menu(&mut self) -> Menu {
        let menu = Menu::new();
        let header = MenuItem::new(format!("KakaoAdHide {VERSION}"), false, None);
        let status = MenuItem::new(self.status_text(), false, None);
        let setup = MenuItem::new("설정하기…", self.status != Status::Serving, None);
        let update = MenuItem::new("업데이트", false, None);
        let changes = MenuItem::new("변경 내역 보기", false, None);
        let skip = MenuItem::new("이 버전 건너뛰기", false, None);
        let check = MenuItem::new("업데이트 확인", true, None);
        let auto_check = CheckMenuItem::new("업데이트 자동 확인", true, updater::enabled(), None);
        let login = CheckMenuItem::new("로그인할 때 실행", cfg!(target_os = "macos"), platform::login_item_enabled(), None);
        let uninstall = MenuItem::new("KakaoAdHide 제거…", true, None);
        let quit = MenuItem::new("종료", true, None);
        let _ = menu.append_items(&[&header, &status]);
        if self.status != Status::Serving { let _ = menu.append(&setup); }
        let _ = menu.append(&PredefinedMenuItem::separator());
        if let Some(f) = &self.pending {
            update.set_text(format!("{}(으)로 업데이트", f.version));
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
            if !agreed && !confirm_label("KakaoAdHide 설정", CONSENT, "설정하기", "나중에") {
                self.status = Status::NotSetUp;
                return;
            }
            platform::set("Consented", Some("1"));
            if let Err(e) = setup::complete(&m) {
                self.status = Status::NotSetUp;
                message("설정을 마치지 못했습니다", &e);
                return;
            }
        }
        self.status = setup::serve();
        if let Status::PortBusy(e) = &self.status {
            message("광고 칸을 비우지 못했습니다", &format!("443 포트를 다른 프로그램이 쓰고 있습니다.\n\n{e}"));
        }
    }

    fn check_updates(&self, interactive: bool) {
        let proxy = self.proxy.clone();
        std::thread::spawn(move || { let _ = proxy.send_event(UserEvent::UpdateChecked(updater::check(), interactive)); });
    }

    fn install(&mut self, f: Feed) -> bool {
        if !platform::can_update_in_place() {
            message("업데이트를 설치하지 않았습니다", &format!("{}\n\n릴리스 페이지에서 직접 받아 주세요.", updater::InstallError::NotInApplications));
            platform::open_url(&f.page_url());
            return false;
        }
        match platform::install_update(&f) {
            Ok(()) => true,
            Err(updater::InstallError::Verify(e)) => {
                message("업데이트를 설치하지 않았습니다", &format!("새 버전({})을 검증하지 못해 설치하지 않았습니다.\n{e}\n\n릴리스 페이지에서 직접 받아 주세요.", f.version));
                platform::open_url(&f.page_url());
                false
            }
            Err(e) => { message("업데이트를 받지 못했습니다", &format!("인터넷 연결을 확인하고 다시 시도해 주세요.\n\n{e}")); false }
        }
    }

    fn offer(&mut self, f: Feed) -> bool {
        let text = format!("KakaoAdHide {}이 나왔습니다. 지금 업데이트할까요?\n받은 파일의 서명을 확인한 뒤에만 설치합니다.", f.version);
        if confirm_label("업데이트", &text, "업데이트", "나중에") { return self.install(f); }
        false
    }
}

// ---------------------------------------------------------------- event handlers (true: exit)

impl App {
    /// First event: setup and server, KakaoTalk (Windows, started by hand), the tray icon, the blank ad's menu.
    fn init(&mut self, at_sign_in: bool, target: &EventLoopWindowTarget<UserEvent>) {
        platform::first_run();
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
            message("업데이트했습니다", &format!("KakaoAdHide {VERSION}(으)로 업데이트되었습니다.\n(이전 버전 {prev})"));
        }
        #[cfg(any(windows, target_os = "macos"))]
        {
            // the menu opened from the blank ad's button
            self.menu_window = tao::window::WindowBuilder::new().with_visible(false).with_title("KakaoAdHide").build(target).ok();
            if self.menu_window.is_some() {
                let p = self.proxy.clone();
                crate::server::set_menu_handler(menu_token(), move || { let _ = p.send_event(UserEvent::ShowMenu); });
            }
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
        platform::take_foreground(hwnd);
        unsafe { menu.show_context_menu_for_hwnd(hwnd, None); }
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
                    return self.offer(f);
                }
            }
            Ok(Check::UpToDate) => {
                self.pending = None;
                self.refresh();
                if interactive { message("최신 버전입니다", &format!("KakaoAdHide {VERSION}")); }
            }
            Err(e) => if interactive { message("업데이트를 확인하지 못했습니다", &e) },
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
                    if let Err(e) = platform::set_hosts(true) { message("설정을 마치지 못했습니다", &e); }
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
                if let Err(e) = platform::set_login_item(!platform::login_item_enabled()) { message("로그인 항목을 바꾸지 못했습니다", &e); }
                self.refresh();
            }
            Some(Action::Uninstall) => {
                if !confirm_label("KakaoAdHide 제거", "제거할까요? 인증서, hosts 파일에 넣은 줄, 설정을 지우고 앱을 삭제합니다. 카카오톡은 그대로입니다.", "제거", "취소") {
                    return false;
                }
                match platform::uninstall(false) {
                    Ok(()) => if cfg!(target_os = "macos") { message("제거했습니다", "KakaoAdHide를 지웠습니다.") },
                    Err(e) => message("일부를 정리하지 못했습니다", &e),
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

/// The tray icon, drawn here (no image files): an ad banner with a slash; a dot when an update is waiting, an
/// outline only while not working. macOS shows it as a template (black, tinted by the system).
fn icon(update: bool, inactive: bool) -> Icon {
    const S: usize = 32;
    let mut px = vec![0u8; S * S * 4];
    let (r, g, b) = if cfg!(target_os = "macos") { (0, 0, 0) } else { (0x3c, 0x1e, 0x1e) };
    let mut put = |x: usize, y: usize, a: u8| {
        if x < S && y < S { let i = (y * S + x) * 4; px[i] = r; px[i + 1] = g; px[i + 2] = b; px[i + 3] = px[i + 3].max(a); }
    };
    // banner: rounded rectangle 4..28 x 9..23
    for y in 9..23 {
        for x in 4..28 {
            let edge = y == 9 || y == 22 || x == 4 || x == 27 || y == 10 || y == 21 || x == 5 || x == 26;
            let corner = !(6..=25).contains(&x) && !(11..=20).contains(&y) && ((x as i32 - 15).abs() + (y as i32 - 16).abs() > 18);
            if corner { continue; }
            if edge || !inactive { put(x, y, if edge { 255 } else { 90 }); }
        }
    }
    // slash
    for i in 0..26 {
        let x = 3 + i; let y = 28 - i;
        for d in 0..3 { put(x + d, y, 255); }
    }
    if update { for y in 0..9 { for x in 23..32 { if (x as i32 - 27).pow(2) + (y as i32 - 4).pow(2) <= 16 { put(x, y, 255); } } } }
    Icon::from_rgba(px, S as u32, S as u32).expect("icon")
}
