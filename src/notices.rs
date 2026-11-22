//! Everything the app says to the user, in one place for both platforms: notifications, questions and errors,
//! the menu labels and the status line. The rest of the code picks one of these and shows it with `ui::show`,
//! `ui::ask` or `ui::error`; how it's shown (a Windows notification, a message box) is up to the platform.
//!
//! Style: short titles ("업데이트 완료", "설정 실패"), bodies of at most two lines, everyday words rather than
//! technical ones. Error details (why a file didn't verify, which program holds port 443) come from where the
//! error happened and are put on the second line.

use crate::VERSION;

/// Something to know, nothing to decide: a notification where the platform has them.
pub struct Notice { pub title: String, pub body: String }

/// A question with two buttons; `ok` is the one that does something.
pub struct Ask { pub title: String, pub body: String, pub ok: &'static str, pub cancel: &'static str }

/// Something went wrong and the user should know (a message box, not a notification that could be missed).
pub struct Problem { pub title: String, pub body: String }

fn notice(title: &str, body: String) -> Notice { Notice { title: title.into(), body } }
fn problem(title: &str, body: String) -> Problem { Problem { title: title.into(), body } }

// ---------------------------------------------------------------- notifications

pub fn updated(previous: &str) -> Notice { notice("업데이트 완료", format!("KakaoAdHide {VERSION}\n이전 버전 {previous}")) }
pub fn new_version(version: &str) -> Notice { notice("새 버전", format!("KakaoAdHide {version}\n눌러서 업데이트")) }
pub fn up_to_date() -> Notice { notice("최신 버전", format!("KakaoAdHide {VERSION}")) }
pub fn installed() -> Notice { notice("설치 완료", "로그인하면 함께 켜집니다\n메뉴: 카카오톡 빈 칸 왼쪽 버튼".into()) }
pub fn removed() -> Notice { notice("제거 완료", "KakaoAdHide를 지웠습니다".into()) }

// ---------------------------------------------------------------- questions

pub fn first_setup() -> Ask {
    Ask {
        title: "처음 설정".into(),
        body: "카카오톡 광고 칸을 비우려면 처음 한 번 설정이 필요합니다.\n\n\
• 이 컴퓨터 전용 인증서를 만들어 등록합니다. 카카오톡 광고 주소 두 개(serv.ds.kakao.com, display.ad.daum.net)에만 \
쓸 수 있고, 다른 사이트에는 쓸 수 없습니다.\n\
• hosts 파일에 광고 주소를 이 컴퓨터로 보내는 두 줄을 넣습니다(관리자 권한 필요).\n\
• 그러면 광고 대신 빈 칸이 보입니다. 광고 서버에는 접속하지 않습니다.\n\n\
같은 광고 주소를 쓰는 웹사이트(다음 등)의 광고도 함께 안 보일 수 있습니다. 제거하면 모두 원래대로 돌아갑니다."
            .into(),
        ok: "설정하기",
        cancel: "나중에",
    }
}

pub fn offer_update(version: &str) -> Ask {
    Ask { title: format!("새 버전 {version}"), body: "지금 업데이트할까요?".into(), ok: "업데이트", cancel: "나중에" }
}

pub fn uninstall() -> Ask {
    Ask {
        title: "제거".into(),
        body: "KakaoAdHide를 제거할까요?\n이 앱이 바꾼 것을 모두 원래대로 돌려놓습니다. 카카오톡은 그대로입니다.".into(),
        ok: "제거",
        cancel: "취소",
    }
}

/// The downloaded update didn't verify; `ok` opens the release page.
pub fn update_unverified() -> Ask {
    Ask {
        title: "업데이트 중단".into(),
        body: "받은 파일이 정식 파일인지 확인하지 못했습니다.\n릴리스 페이지에서 직접 받아 주세요.".into(),
        ok: "릴리스 페이지 열기",
        cancel: "닫기",
    }
}

/// macOS: the app can't replace itself; `ok` opens the release page.
pub fn update_not_possible() -> Ask {
    Ask {
        title: "업데이트 불가".into(),
        body: "응용 프로그램 폴더 밖이거나 권한이 없습니다.\n릴리스 페이지에서 직접 받아 주세요.".into(),
        ok: "릴리스 페이지 열기",
        cancel: "닫기",
    }
}

/// macOS: started from outside the Applications folder.
pub fn move_to_applications() -> Ask {
    Ask {
        title: "응용 프로그램 폴더로 옮기기".into(),
        body: "응용 프로그램 폴더에 있어야 로그인할 때 켜지고 업데이트됩니다.".into(),
        ok: "옮기기",
        cancel: "그대로 실행",
    }
}

// ---------------------------------------------------------------- problems

pub fn check_failed(why: &str) -> Problem { problem("업데이트 확인 실패", why.into()) }
pub fn download_failed(why: &str) -> Problem { problem("업데이트 실패", format!("인터넷 연결을 확인해 주세요.\n{why}")) }
pub fn setup_failed(why: &str) -> Problem { problem("설정 실패", why.into()) }
pub fn port_busy(who: &str) -> Problem { problem("443 포트 사용 중", format!("다른 프로그램이 443 포트를 쓰고 있습니다.\n{who}")) }
pub fn failed(why: &str) -> Problem { problem("오류", why.into()) }
pub fn uninstall_incomplete(left: &str) -> Problem { problem("제거 중 오류", format!("일부를 정리하지 못했습니다.\n{left}")) }
pub fn move_failed(folder: &str) -> Problem { problem("옮기기 실패", format!("{folder}에 직접 끌어다 놓아 주세요.")) }
pub fn login_item_failed(why: &str) -> Problem { problem("로그인 항목 변경 실패", why.into()) }

// ---------------------------------------------------------------- menu and status

pub mod menu {
    pub const SETUP: &str = "설정하기…";
    pub const UPDATE: &str = "업데이트";
    pub const CHANGES: &str = "변경 내역 보기";
    pub const SKIP: &str = "이 버전 건너뛰기";
    pub const CHECK: &str = "업데이트 확인";
    pub const AUTO_CHECK: &str = "업데이트 자동 확인";
    pub const LOGIN: &str = "로그인할 때 실행";
    pub const UNINSTALL: &str = "KakaoAdHide 제거…";
    pub const QUIT: &str = "종료";
    pub fn update_to(version: &str) -> String { format!("{version}(으)로 업데이트") }
}

pub mod status {
    pub const SERVING: &str = "광고 칸을 비우는 중";
    pub const NOT_SET_UP: &str = "설정이 필요합니다";
    pub const PORT_BUSY: &str = "443 포트를 다른 프로그램이 쓰고 있습니다";
    pub const NOT_ROUTED: &str = "광고 주소가 이 컴퓨터로 오지 않습니다";
    pub fn failed(why: &str) -> String { format!("오류: {why}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> usize { s.lines().count() }

    #[test]
    fn notifications_are_short() {
        for n in [updated("1.0.0"), new_version("9.9.9"), up_to_date(), installed(), removed()] {
            assert!(n.title.chars().count() <= 10, "{}", n.title);
            assert!(lines(&n.body) <= 2, "{}", n.body);
        }
    }

    #[test]
    fn questions_and_problems_are_short() {
        for a in [offer_update("9.9.9"), uninstall(), update_unverified(), update_not_possible(), move_to_applications()] {
            assert!(a.title.chars().count() <= 16 && lines(&a.body) <= 2, "{}", a.title);
        }
        for p in [check_failed("x"), download_failed("x"), setup_failed("x"), port_busy("x"), failed("x"),
                  uninstall_incomplete("x"), move_failed("x"), login_item_failed("x")] {
            assert!(p.title.chars().count() <= 12 && lines(&p.body) <= 2, "{}", p.title);
        }
    }
}
