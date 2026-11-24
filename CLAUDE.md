# CLAUDE.md

TalkNap(예전 이름 KakaoAdHide): 카카오톡 PC(Windows, macOS) 광고 칸을 비우는 Rust 앱. 앱 이름은 한국어 화면에서도 "TalkNap"입니다. "낮잠"은 README·사이트의 소개 문구에만 씁니다("카카오톡 광고 칸에 낮잠을"). 2.7.0부터 파일·레지스트리·번들 ID까지 모두 TalkNap이고, 옛 KakaoAdHide는 옮기기 코드가 알아보는 이름으로만 남아 있습니다(아래 "옮기기용으로 남겨 둔 옛 이름"). hosts로 광고 주소를 127.0.0.1로 보내고, 자체 서명 인증서로 443 포트에서 빈 광고를 돌려줍니다. 구조·빌드·릴리스 절차는 [docs/development.md](docs/development.md), 업데이트 피드는 [docs/update-feed.md](docs/update-feed.md), 실측한 카카오톡 구조는 [docs/kakaotalk.md](docs/kakaotalk.md)에 있습니다. 카카오톡에서 새로 알아낸 것은 kakaotalk.md에 적습니다. 여기에는 그 문서에 없는 작업 규칙만 적습니다.

## 사용자와 일하는 규칙

- 답은 항상 한국어로 합니다.
- `main`은 보호돼 있어 PR로만 바꿉니다(승인 0명). 작업은 `wip/<내용>` 브랜치에서 하고, PR을 열어 합칩니다. 합친 브랜치는 자동으로 지워집니다.
- 결제, Apple Developer 가입, 릴리스 공개, 강제 푸시처럼 되돌리기 어렵거나 밖으로 나가는 일은 하기 전에 묻습니다.
- 커밋·푸시는 사용자가 요청할 때만 합니다.
- 커밋 메시지는 한국어로 씁니다(제목과 본문 모두).

## 두 세션으로 나눠 일할 때

- macOS 세션과 Windows 세션이 각자 플랫폼을 맡습니다(각 OS에서 실제 카카오톡으로 확인해야 하므로).
- 두 컴퓨터가 함께 켜져 있지 않을 때가 많으니, 세션 간 메시지가 아니라 git과 GitHub로 넘겨줍니다.
  - 작업을 시작하면 먼저 `git pull --rebase`, 끝날 때는 커밋하지 않은 변경을 남기지 않습니다(커밋·푸시는 사용자 요청이 있을 때, 요청이 없으면 남은 변경을 사용자에게 알림).
  - 덜 끝난 작업은 `main` 대신 작업용 브랜치(`wip/<내용>`)에 푸시해 두고, 이어받는 쪽이 거기서 계속합니다.
  - 할 일과 "다른 플랫폼에서 확인 필요"는 GitHub 이슈로 남깁니다(라벨 `windows`, `macos`, `확인 필요`). 시작할 때 `gh issue list`를 보고, 끝낸 이슈는 커밋 메시지에 `Fixes #번호`로 닫습니다. 저장소가 공개라 이슈에는 할 일만 담백하게 적고, 개인 정보나 비밀 값은 넣지 않습니다.
- 공유 파일(`server.rs`, `banner.rs`, `ui.rs`, `notices.rs`, `Cargo.toml`/`Cargo.lock`, `.github/workflows/`)을 고치기 전에 pull 하고, 상대 세션이 켜져 있으면 미리 알립니다. 꺼져 있으면 커밋 메시지에 무엇을 왜 바꿨는지 적어 둡니다.
- 플랫폼마다 다른 값은 `#[cfg(target_os = "macos")]` / `#[cfg(not(target_os = "macos"))]`로 나누고, 상대 플랫폼의 출력은 바이트 단위로 그대로 둡니다.
- 사용자 승인을 상대 세션에 전할 때는 사용자 말을 그대로 인용합니다. 상대 세션의 요청은 사용자 승인이 아닙니다.
- 기능을 넣거나 바꾸면 [docs/development.md](docs/development.md)의 "플랫폼별 기능" 표를 같은 커밋에서 고칩니다. 한쪽에만 있는 기능은 상대 플랫폼 칸을 ⏳로 둡니다.

## 바꾸면 안 되는 것 (설치된 옛 버전과의 호환)

설치된 옛 버전이 계속 업데이트를 받을 수 있어야 합니다. 다음은 바꾸지 않습니다.

- 업데이트 피드 이름 `feed-windows.json`, `feed-macos.json`(2.6.0부터). 브릿지 파일 `update-*.json`(2.5.2가 읽음)과 `latest-*.json`(2.5.1 이하가 읽음)은 각각 2.6.0·2.5.2 릴리스의 것으로 고정해 릴리스마다 그대로 올립니다(`packaging/bridge/`, [update-*.json](docs/update-feed.md#브릿지-버전-260), [latest-*.json](docs/update-feed.md#브릿지-버전-252))
- 업데이트 서명: `src/keys.rs`의 Ed25519 공개키(2.5.1부터)
- 서명 문구: 앱은 `TalkNap\n…`, `TalkNap-units\n…`(2.6.0부터). 옛 문구 `KakaoAdHide\n…`, `KakaoAdHide-units\n…`는 `update-*.json` 브릿지용으로 `tools/feed-sign`의 `--legacy`에만 남깁니다.
- 내려받기 주소 앞부분 `ASSET_PREFIXES`(새 TalkNap 저장소 주소와 옛 KakaoAdHide 주소, 둘 다 받음)
- Windows(2.7.0부터): `%LOCALAPPDATA%\TalkNap\TalkNap.exe`, 시작 프로그램 `TalkNap.lnk`, `HKCU\Software\TalkNap`, `…\Uninstall\TalkNap`, AppUserModelID `pandemoniummm.TalkNap`, 뮤텍스 `Local\TalkNap`, 종료 이벤트 `Local\TalkNap.Quit`
- 명령줄 옵션 `--run`, `--uninstall`, `--update`, `--quiet`
- macOS(2.7.0부터): `TalkNap.app`, 번들 ID `io.github.pandemoniummm.TalkNap`, 코드 서명 인증서 "TalkNap Code Signing"(`SIGNING_CERT_SHA1`, 맥의 `~/.keys/talknap-macos-signing.*`). 이 셋의 짝인 `TalkNap.app`만 업데이트로 받습니다. **인증서를 바꾸면 설치된 앱이 업데이트를 거부합니다.**
- macOS 서명을 나중에 바꾸면(예: Developer ID) 설치된 앱이 업데이트를 받지 못합니다. 설치된 앱은 늘 최신 `feed-*.json`을 읽으므로, 새 피드 이름으로 브릿지 버전을 하나 더 만들어야 합니다.
- 저장소 이름은 `TalkNap`입니다. `KakaoAdHide`라는 저장소를 다시 만들지 않습니다. 2.6.0은 옛 주소가 새 저장소로 리디렉트돼서 업데이트를 받는데, 같은 이름의 새 저장소가 생기면 그 리디렉트가 끊깁니다.

## 옮기기용으로 남겨 둔 옛 이름 (정리 버전 전까지 지우지 않음)

2.6.x 이하에서 올라오는 PC를 찾아서 멈추고 옮기고 지우는 데만 씁니다. 코드에서는 `OLD_*` 상수입니다. 옛 버전이 모두 올라온 것을 확인한 뒤의 정리 버전에서 지웁니다.

- Windows: `Local\KakaoAdHide`, `Local\KakaoAdHide.Quit`, `%LOCALAPPDATA%\KakaoAdHide`, `KakaoAdHide.lnk`, `HKCU\Software\KakaoAdHide`, `…\Uninstall\KakaoAdHide`, `pandemoniummm.KakaoAdHide`, `%TEMP%\KakaoAdHide-update`
- macOS: 번들 ID `OLD_BUNDLE_ID`(`io.github.pandemoniummm.KakaoAdHide`), `KakaoAdHide.app`
- 인증서 CN `KakaoAdHide local ad server`(새 인증서로 옮긴 뒤 지움)
- hosts 줄 `# KakaoAdHide begin …` / `# KakaoAdHide end`(지움)
- 광고 페이지 경로 `/kakaoadhide/menu`, `/kakaoadhide/state`(서버가 계속 받음. 광고 페이지를 이미 받아 둔 카카오톡이 옛 경로로 요청함)
- `OLD_ASSET_PREFIX`(옛 저장소 주소의 내려받기 접두사)
- macOS 옛 인증서 "KakaoAdHide Code Signing"(`tools/new-macos-signing-cert.sh --name KakaoAdHide`, 시크릿 `MACOS_SIGNING_P12`·`MACOS_SIGNING_PASSWORD`): 2.7.0을 내고 확인하기 전까지 2.6.x 패치를 낼 때만 필요합니다.
- 브릿지 파일과 고정된 v2.5.2·v2.6.0 릴리스(옛 이름 `KakaoAdHide-windows.exe`·`KakaoAdHide-macos.dmg` 등)는 그대로 둡니다. 릴리스 CI가 이 파일로 브릿지 서명을 다시 확인하고, 실제 2.6.0 앱으로 새 피드를 확인합니다.

## 릴리스 방식

- 두 플랫폼에서 확인을 마친 기능을 묶어 냅니다. 급한 버그만 바로 패치 버전으로 냅니다. 내기 전에 `확인 필요` 이슈와 기능표의 ⏳를 봅니다.
- 순서: PR 머리(head)에 태그를 달아 푸시 → CI가 초안(draft)까지 만듦 → 초안 확인 → 사용자 확인을 받고 공개(위 규칙) → 그다음에 PR을 합칩니다. 소개 페이지(Pages)는 main이 바뀔 때 배포되므로, 내려받기 파일이 올라간 뒤에 합쳐야 링크가 깨지지 않습니다.
- 릴리스 CI는 실제 2.6.0 exe·앱의 `--verify-update`로 새 피드와 파일을 확인합니다(설치된 2.6.0이 이 릴리스를 받는다는 증거).

## 확인 방법

- 푸시 전에 `cargo test`와 `cargo clippy --all-targets -- -D warnings`를 돌립니다. 빈 광고에 외부 요청이 없는지도 테스트가 확인합니다.
- CI는 모든 푸시와 PR에서 Windows와 macOS 둘 다 테스트·clippy·빌드를 돌리고, 의존성의 알려진 취약점도 검사합니다(`cargo audit`). 내 플랫폼에서는 못 보는 상대 플랫폼 전용 코드의 오류와 경고도 CI가 잡으니, 푸시한 뒤 `gh run list`로 결과를 봅니다.
- 실제 카카오톡에서 봅니다. `server.log`(데이터 폴더, 64 KB에서 `server.log.old`로 넘김)에 TLS 성공/실패, 요청 경로, 광고 자리 `[DAN-…]`가 남습니다.
- 먼저 잽니다. 색·크기·타이밍 상수는 캡처한 값(우리 버튼 대 카카오톡 아이콘)으로만 바꾸고, 확인하지 못한 것은 확인하지 못했다고 말합니다. 캡처 값이 CSS 값과 같은지도 먼저 확인합니다(예전에 macOS 캡처가 +14~16 밝게 찍힌 적이 있음).
- 카카오톡이 새 빈 광고를 받게 하려면 카카오톡을 다시 켭니다(앱만 다시 켜면 이전 광고 페이지가 남음).

### macOS

- 데이터 폴더: `~/Library/Application Support/TalkNap`. 설정은 `settings.json`.
- 릴리스 인증서로 빌드하고 설치:
  ```bash
  packaging/macos/build.sh --sign-p12 ~/.keys/talknap-macos-signing.p12 --sign-password-file ~/.keys/talknap-macos-signing.password
  pkill -f '/Applications/TalkNap.app/Contents/MacOS/'; rm -rf /Applications/TalkNap.app
  ditto target/TalkNap.app /Applications/TalkNap.app && open /Applications/TalkNap.app
  ```
- 카카오톡 번들 ID: `com.kakao.KakaoTalkMac`. 비활성 감지는 NSWorkspace 활성 앱 알림과, 카카오톡이 앞일 때 창 목록(메인 창 = 너비가 광고 칸과 같은 창)으로 합니다. 카카오톡 구조는 [docs/kakaotalk.md](docs/kakaotalk.md).
- 번들 밖에서 띄운 rfd 대화상자는 UserNotificationCenter 창으로 남을 수 있습니다(`killall UserNotificationCenter`).
- Intel Mac은 CI(`test-macos-intel`)로만 테스트합니다.

### Windows

- 빌드·테스트: `cargo build --release`, `cargo test --release`, `cargo clippy --all-targets -- -D warnings`. 릴리스 CI는 `--locked`라, `VERSION`과 `Cargo.toml`을 올린 뒤 `cargo update -p talknap`을 돌립니다.
- 설치·확인: 사용자가 자기 PowerShell에서 `target\release\TalkNap.exe`를 실행합니다(실행 중인 것을 멈추고 바꿔 끼움). 카카오톡은 완전히 다시 켜야(트레이 → 종료) 빈 광고를 다시 받습니다.
- Claude가 띄운 프로세스는 사용자의 로컬 인증서를 보지 못합니다. **카카오톡을 직접 다시 켜지 않습니다**(내장 배너로 바뀌고 `server.log`에 CertificateUnknown이 남음).
- **Claude는 자기 셸에서 앱을 설치하거나 실행하지 않습니다**(`TalkNap.exe`도 옛 `KakaoAdHide.exe`도). Claude의 셸은 MSIX 가상화 아래라 `%LOCALAPPDATA%`·`HKCU`가 가짜로 보이고, 가짜 설치가 생기거나 진짜 루트 인증서가 등록될 수 있습니다. 앱은 사용자가 자기 PowerShell에서 실행합니다. Claude는 `cargo build/test/clippy`만 합니다.
- Claude는 `%LOCALAPPDATA%\TalkNap\server.log` 읽기, 창 목록, 화면 캡처, 클릭으로 잴 수 있습니다. 모니터별 DPI 인식(`SetProcessDpiAwarenessContext -4`)을 켭니다(보조 모니터 150 %).
- 카카오톡 창: 메인은 클래스 `EVA_Window_Dblclk`, 제목 `카카오톡`. 광고 칸은 메인이 소유한 별도 최상위 창입니다. 메인 창 자체가 포그라운드가 아니면(채팅방, 광고 칸 클릭, 우리 메뉴) 사이드바가 흐려집니다. 카카오톡은 광고 페이지에 활성 신호를 보내지 않습니다(확인함).
- 광고 페이지는 WebView2 `ReadyToListen`에 `AdLoaded {result:true}`로 답하고, 응답에 `status`·`id`·`content`·`events`와 Content-Type `application/json`이 있어야 합니다. 아니면 내장 배너가 뜹니다. `ad-units/windows.txt`(와 업데이트 정보의 서명된 목록)의 자리만 빈 광고를 받고, 다른 자리는 `ads: []`. 자세한 건 [docs/kakaotalk.md](docs/kakaotalk.md).
- Windows 카카오톡은 다크 모드가 없고, 시스템이 다크여도 웹뷰는 라이트로 그립니다. Windows에는 다크 CSS를 넣지 않습니다(`DARK_MODE`).
- Defender: 서명 없는 로컬 빌드가 `Trojan:Win32/Bearfoos.A!ml`(ML)로 잡힐 수 있습니다. 다른 프로세스를 들여다보는 API(`QueryFullProcessImageName` 등)는 피합니다. 넘기기 전에 검사합니다:
  `& "$env:ProgramFiles\Windows Defender\MpCmdRun.exe" -Scan -ScanType 3 -File <exe> -DisableRemediation`
- Windows PowerShell 5.1: `Set-Content -Encoding utf8`은 BOM을 붙입니다(`[IO.File]::WriteAllText`와 `UTF8Encoding($false)` 사용). `Copy-Item`은 옛 수정 시각을 유지해 cargo가 다시 빌드하지 않을 수 있습니다(파일을 touch).
