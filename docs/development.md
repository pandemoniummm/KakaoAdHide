# 개발 문서

사용 방법은 [README](../README.md)를 보세요. 이 문서는 빌드와 릴리스를 다룹니다.

## 구조

Rust 하나로 두 플랫폼을 만듭니다.

| 경로 | 내용 |
|---|---|
| `src/` | 공통: 로컬 HTTPS 서버(`server.rs`), 빈 광고(`banner.rs`), 인증서(`certs.rs`), hosts(`hosts.rs`), 설정 단계(`setup.rs`), 업데이트(`feed.rs`, `updater.rs`), 트레이·메뉴 막대(`ui.rs`), 사용자에게 보이는 모든 문구(`notices.rs`) |
| `src/platform/` | `windows.rs`(설치·레지스트리·UAC·인증서 저장소), `macos.rs`(앱 번들·키체인·LaunchAgent) |
| `packaging/` | `windows/build.ps1`(`dist/TalkNap.exe`), `macos/build.sh`(`TalkNap.app` 번들·서명·`dist/TalkNap.dmg`), `macos/Info.plist` |
| `tools/` | 업데이트 정보 만들기·서명(`feed-sign/`, Ed25519), macOS 코드 서명 인증서(`new-macos-signing-cert.sh`), 이 PC에 남은 옛 이름 흔적 찾기(`find-old-traces.ps1`, 읽기만 함) |
| `packaging/bridge/` | 옛 버전이 읽는 브릿지 파일: `latest-*.json`(2.5.1 이하, 2.5.2 브릿지 버전)과 `update-*.json`(2.5.2, 2.6.0 브릿지 버전). 고치지 않음 |
| `assets/` | 아이콘 원본: 앱 아이콘(`icon.svg`), 선 아이콘(`icon-line.svg`, 사이드바 버튼과 트레이 모양) |
| `tools/icon-gen/` | `icon.svg`에서 `packaging/windows/app.ico`, `packaging/macos/AppIcon.icns`, `assets/icon-256.png`을, `dmg-background.svg`에서 dmg 창 배경(`packaging/macos/dmg-background*.png`)을 만드는 도구 |
| `ad-units/` | 빈 광고를 줄 하단 배너의 광고 자리 ID(플랫폼별). 앱에 들어가고 업데이트 정보에도 서명해 실음 |
| `docs/update-feed.md` | 업데이트 정보 형식과 서명 |
| `site/` | 소개·다운로드 페이지(GitHub Pages, `.github/workflows/pages.yml`이 main에서 배포) |
| `docs/kakaotalk.md` | 카카오톡의 광고 요청·응답·창 구조(실측) |
| `VERSION` | 두 플랫폼 공통 버전 (`Cargo.toml`의 `version`과 같게) |

### 빈 광고 (`banner.rs`)

- 카카오톡은 `serv.ds.kakao.com/sdk/banner`에 광고 목록(JSON)을 요청하고, 첫 광고의 HTML을 하단 웹뷰에 그립니다.
- 빈 목록을 주면 카카오톡이 내장 배너로 바꿔 버리므로, 배경색만 칠한 "광고" 하나를 돌려줍니다.
- Windows 카카오톡은 더 깐깐합니다.
  - `status`, `id`, 광고의 `content`와 `events`가 있어야 받아들입니다([kakaotalk.md](kakaotalk.md#응답)). 이벤트 링크는 모두 로컬 서버(`/ev/*`, 204)를 가리킵니다.
  - WebView2로 `ReadyToListen`을 보내고 `AdLoaded {result:true}` 응답을 기다립니다. 없으면 실패로 보고 대체 배너를 띄웁니다.
- 사이드바 부분의 폭은 페이지가 직접 계산합니다: `광고 칸 높이 × SIDEBAR_PER_SLOT`(Windows 122/188, macOS 78/100). Windows 웹뷰의 CSS 픽셀은 카카오톡 배율을 따라가지 않지만, 사이드바와 광고 칸은 함께 커지기 때문입니다.
- 사이드바 부분에 메뉴 버튼을 그립니다. 누르면 `/talknap/menu?t=<토큰>`을 요청하고, 앱이 커서 위치에 메뉴를 띄웁니다. 토큰은 처음 한 번 만들어 저장합니다(Windows는 레지스트리 `MenuToken`).
- 버튼은 카카오톡 메인 창이 앞에 있지 않으면 흐려집니다(채팅방·설정 창이 앞이어도). 웹뷰는 창 상태를 모르므로 앱이 알아내고, 빈 광고가 `/talknap/state?t=<토큰>`에 SSE 연결 하나를 열어 상태가 바뀔 때 받습니다(`server::set_kakao_active`). 서버는 옛 경로 `/kakaoadhide/menu`, `/kakaoadhide/state`도 계속 받습니다(이미 받아 둔 옛 광고 페이지용. 정리 버전에서 지움).
  - Windows: 포그라운드 변경 이벤트(`SetWinEventHook`)와 메인 창의 클래스·제목.
  - macOS: 활성 앱 알림(NSWorkspace)으로 카카오톡이 앞인지 알고, 카카오톡이 앞인 동안만 0.25초마다 창 목록(`CGWindowListCopyWindowInfo`)의 맨 앞 창을 봅니다. 창 제목은 화면 기록 권한이 있어야 보이므로, 빈 광고가 SSE 주소에 붙여 보내는 광고 칸 폭(`&w=`, 메인 창 폭과 같음)으로 메인 창을 찾습니다.
- 테스트가 빈 광고에 외부 요청이 없는지 확인합니다(메뉴 버튼의 로컬 요청과 상태 스트림, 두 로컬 주소만 허용).
- 광고 자리는 요청의 `id`로 구분합니다(`server.log`에 `[DAN-…]`로 남음). Windows는 하단 배너(`ad-units/windows.txt`)에만 빈 광고를 주고, 다른 자리(트레이 근처 팝업 등)에는 빈 목록을 줘서 뜨지 않게 합니다. 카카오톡 업데이트로 하단 배너 `id`가 바뀌면 내장 배너가 다시 보이니, 로그를 보고 목록에 추가하고 `units` 워크플로로 업데이트 정보에 실어 보냅니다([update-feed.md](update-feed.md#광고-자리-id-목록)). macOS는 아직 모든 자리에 빈 광고를 줍니다.

## 플랫폼별 기능

한쪽 플랫폼에서 기능을 넣거나 바꾸면 이 표도 고칩니다. 릴리스 노트를 쓸 때 이 표를 봅니다. ✅ 됨, ⏳ 아직, ➖ 해당 없음.

| 기능 | Windows | macOS | 비고 |
|---|---|---|---|
| 하단 광고 칸 비우기 | ✅ | ✅ | |
| 다른 광고 자리(팝업 등) 막기 | ✅ | ⏳ | macOS는 모든 자리에 빈 광고를 줌(다른 자리 요청을 아직 못 봄) |
| 하단 배너 ID를 업데이트 정보로 받기 | ✅ | ⏳ | macOS는 모든 자리에 빈 광고를 주니 아직 필요 없음 |
| 빈 광고 칸의 메뉴 버튼 | ✅ | ✅ | |
| 카카오톡 비활성일 때 버튼 흐리게 | ✅ | ✅ | 메인 창이 앞일 때만 진함(채팅방·설정 창이 앞이면 흐림) |
| 버튼 모양 | 위아래 윤곽선 | 위 채움, 아래 윤곽선 | 각 카카오톡 아이콘 모양에 맞춤 |
| 다크 모드 | ➖ | ✅ | Windows 카카오톡에 다크 모드가 없음 |
| 알릴 일은 시스템 알림으로 | ✅ | ✅ | 알림을 못 띄우면(권한 거절 등) 대화상자. 제거 확인·오류는 늘 대화상자 |
| 트레이 / 메뉴 막대 아이콘 | ✅ | ✅ | |
| hosts가 풀리면 알림으로 다시 비우기 | ✅ | ✅ | 10분마다 확인. 알림이나 [다시 비우기]를 누르면 관리자 권한을 묻고 다시 씀. 권한을 거절하면 "광고 칸 비우기 실패" |
| 업데이트 확인·설치 | ✅ | ✅ | |
| 새 버전을 알림 버튼으로 묻기 | ✅ | ✅ | 알림이나 [업데이트]를 누르면 바로 설치 |
| 설치·업데이트 후 TalkNap 다시 실행 | ✅ | ✅ | |
| 이름이 바뀐 앱(TalkNap)으로 업데이트 | ✅ | ✅ | 2.6.0 앱이 새 피드를 읽고 `TalkNap.exe`·`TalkNap.dmg`를 받음(릴리스 CI가 2.6.0 앱의 `--verify-update`로 증명). Windows(2.7.0, 실기 확인 전): 새 이름으로 설치하며 옛 설치를 옮기고 지움. macOS: dmg의 `TalkNap.app`을 새 번들 ID·새 인증서로 확인하고 `KakaoAdHide.app` 자리를 바꿈 |
| 옛 이름 흔적 지우기 | ✅ | ✅ | 2.7.0. Windows는 2.6.0에서 업데이트(흉내)·실행 중 직접 실행 두 경로와 각 확인 창 거절 뒤 다시 켜기로 실기 확인(알림 설정 키 `Notifications\Settings\pandemoniummm.KakaoAdHide`는 Windows가 값 하나로 되살림), macOS는 2.6.0에서 업데이트·dmg 직접 설치 두 경로로 실기 확인. Windows: 옛 설치 폴더·바로가기·레지스트리·인증서(옮긴 뒤)·hosts 줄, 시스템이 남긴 옛 이름 기록(`tools/find-old-traces.ps1`로 확인). macOS: 옛 앱·LaunchAgent·데이터 폴더·키체인 인증서·hosts 줄. 직접 내려받은 설치 파일, 최근 문서, 카카오톡 캐시, Amcache, 이벤트 로그는 지우지 않음. macOS의 "백그라운드에서 허용" 끔은 앱이 옮길 수 없음 |
| 로그인 항목 이어 주기 | ✅ | ✅ | 2.7.0. Windows는 바로가기를 새 이름으로 바꾸고 작업 관리자에서 끈 상태가 이어지는 것, 로그아웃·로그인으로 자동 실행을 실기 확인. 제거하면 작업 관리자의 켜기/끄기 기록도 지움. macOS(로그아웃·로그인으로 실기 확인): 옛 LaunchAgent가 켜져 있었으면 새 것을 쓴 뒤 옛 것을 지움. LaunchAgent는 `open -a` 대신 앱 실행 파일을 직접 실행해 "백그라운드에서 허용"·알림에 `open`이 아니라 TalkNap으로 보임(아이콘은 팀 ID가 없어 실행 파일 모양). 앱이 launchd 작업 자체가 되므로 `AbandonProcessGroup`으로 업데이트 바꿔 끼우기 스크립트가 함께 죽지 않게 함. Windows: 시작 프로그램 바로가기를 새 이름으로 다시 만들고, 작업 관리자에서 끈 것도 옮김 |
| 직접 실행할 때 카카오톡 켜기(꺼져 있으면, 새 빈 광고 받기) | ✅ | ✅ | 로그인할 때는 카카오톡 자체 설정을 따름. 이미 켜져 있으면 다시 켜지 않음 |
| 메뉴에서 카카오톡 다시 시작(새 빈 광고 받기) | ✅ | ✅ | Windows는 카카오톡 프로세스를 끝내고 다시 켬(창을 닫으면 트레이로 숨기만 함) |

## 아이콘

`assets/icon.svg`를 고친 뒤 아이콘 파일을 다시 만듭니다(결과물은 커밋합니다. 앱 빌드에는 이 도구가 필요 없습니다).

```bash
cargo run --release --manifest-path tools/icon-gen/Cargo.toml
```

소개 페이지(`site/`)는 `icon.svg`와 `icon-256.png` 사본을 씁니다. 아이콘을 바꾸면 `site/`에도 복사합니다.

사이드바 버튼(`banner.rs`)과 트레이 아이콘(`ui.rs`의 `icon_rgba`)은 `assets/icon-line.svg`와 같은 모양을 코드로 그립니다. 모양을 바꾸면 셋을 함께 고칩니다.

## 빌드

```bash
cargo test                       # 공통 테스트
packaging/macos/build.sh --dmg   # macOS: target/TalkNap.app, dist/TalkNap.dmg
```

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1   # Windows: dist\TalkNap.exe
```

macOS에서 그냥 빌드하면 ad-hoc 서명이라 릴리스 앱과 서로 업데이트할 수 없습니다. 릴리스 인증서로 서명하려면 `--sign-p12 ~/.keys/talknap-macos-signing.p12 --sign-password-file ~/.keys/talknap-macos-signing.password`를 붙입니다.

## 릴리스

### 처음 한 번: 업데이트 서명 키

```powershell
cargo run --release --manifest-path tools/feed-sign/Cargo.toml -- keygen $HOME\.keys\signing-ed25519-primary.key
cargo run --release --manifest-path tools/feed-sign/Cargo.toml -- keygen $HOME\.keys\signing-ed25519-backup.key
Get-Content $HOME\.keys\signing-ed25519-primary.key -Raw | gh secret set SIGNING_KEY_ED25519
```

- Ed25519 키 두 개(주 키, 예비 키)를 저장소 밖에 만듭니다. 출력되는 공개키 두 개를 `src/keys.rs`에 넣습니다(개인키는 출력하지 않음).
- 주 키의 개인키를 `SIGNING_KEY_ED25519` 시크릿으로 저장합니다(`gh` 로그인 필요).
- 키를 바꿀 때는 새 공개키를 넣은 버전을 먼저 낸 뒤에 새 키로 서명합니다. 설치된 앱은 자기 안의 공개키만 믿습니다.
- **개인키 두 개를 반드시 백업하세요**(비밀번호 관리자, 암호화한 USB 등). GitHub 시크릿은 다시 꺼내 볼 수 없습니다.
  예비 키는 릴리스에 쓰지 않으니 백업한 뒤 PC에서 지워 두세요. 주 키를 잃어버리면 예비 키로 서명한 업데이트로 키를 바꿀 수 있습니다.
- 바뀐 `src/keys.rs`(공개키만 들어 있음)를 커밋합니다.

### 처음 한 번: macOS 코드 서명 인증서

```bash
tools/new-macos-signing-cert.sh
tools/new-macos-signing-cert.sh --set-secret
```

- 자체 서명 인증서 "TalkNap Code Signing"(무료)을 저장소 밖(`~/.keys/talknap-macos-signing.p12`, `.password`)에 만들고 SHA-1을 출력합니다. 그 값이 `src/platform/macos.rs`의 `SIGNING_CERT_SHA1`에 들어갑니다.
- `--set-secret`은 `MACOS_SIGNING_P12_TALKNAP`, `MACOS_SIGNING_PASSWORD_TALKNAP` 시크릿을 저장합니다(`gh` 로그인 필요).
- **두 파일을 반드시 백업하세요.** 인증서를 바꾸면 설치된 앱이 업데이트를 거부합니다.
- 옛 인증서 "KakaoAdHide Code Signing"은 `--name KakaoAdHide`로 만들고(시크릿 `MACOS_SIGNING_P12`, `MACOS_SIGNING_PASSWORD`), 2.7.0을 내고 확인하기 전에 2.6.x 패치를 낼 때만 필요합니다. 두 파일을 그때까지 보관합니다.

### 버전마다

버전 규칙: 버그 수정은 셋째 자리(2.0.**1**), 새 기능은 둘째 자리(2.**1**.0), 방식이 크게 바뀌면 첫째 자리(**3**.0.0).

1. 버전을 올립니다.
   - `VERSION` 파일과 `Cargo.toml`의 `version`을 새 버전으로 바꿉니다.
   - `Cargo.lock`도 맞춥니다. 빠뜨리면 CI(`--locked`)가 실패합니다.
     ```powershell
     cargo update -p talknap
     ```
2. 커밋하고 브랜치를 푸시한 뒤 PR을 열어 `main`에 합칩니다. `main`은 보호돼 있어 PR로만 바뀝니다(승인 0명). `main`에 합쳐도 릴리스는 태그를 달 때만 만들어집니다.
   ```powershell
   git switch -c chore/release-2.1.0; git add -A; git commit -m "chore: 2.1.0"; git push origin chore/release-2.1.0; gh pr create --fill
   ```
3. 합친 뒤 `main`의 그 커밋에 같은 이름의 태그를 올립니다. 태그 이름(`v2.1.0`)과 `VERSION`(`2.1.0`)이 다르면 CI가 멈춥니다.
   ```powershell
   git switch main; git pull; git tag -a v2.1.0 -m v2.1.0; git push origin v2.1.0
   ```
4. GitHub Actions가 두 플랫폼 빌드 → 업데이트 정보 서명 → 각 플랫폼 앱으로 서명 검증 → **초안(draft) 릴리스** 생성까지 합니다.
   올라가는 파일: `TalkNap.exe`, `feed-windows.json`, `update-windows.json`, `latest-windows.json`, `TalkNap.dmg`, `feed-macos.json`, `update-macos.json`, `latest-macos.json`, `SHA256SUMS.txt`
   - `update-*.json`(2.5.2가 읽음)과 `latest-*.json`(2.5.1 이하가 읽음)은 브릿지 파일로, `packaging/bridge/`의 파일을 그대로 올립니다([update-feed.md](update-feed.md#브릿지-버전-252)). 옛 이름 사본(`KakaoAdHide-*.exe` 등)은 올리지 않습니다.
   - CI는 실제 v2.6.0 릴리스의 `KakaoAdHide-windows.exe`, `KakaoAdHide-macos.dmg`를 내려받아 `--verify-update`로 새 피드와 새 파일을 확인합니다. 이 확인이 실패하면 설치된 2.6.0이 이 릴리스를 받지 못한다는 뜻이니 공개하지 않습니다.
5. 초안을 확인하고, 사용자 확인을 받은 뒤 릴리스 노트를 쓰고 **Publish release**를 누르면, 그때부터 사용자에게 업데이트 알림이 갑니다.
6. 소개 페이지(`site/`)는 `main`이 바뀔 때 바로 배포되고, README는 저장소 첫 화면에 바로 보입니다. 아직 내지 않은 릴리스 파일이나 기능을 안내하는 변경(내려받기 파일 이름, 새 기능 설명 등)은 릴리스를 공개한 다음에 합칩니다.

공개 전에 태그를 다시 붙여야 하면(CI 실패를 고친 뒤 등):

```powershell
git push origin :refs/tags/v2.1.0; git tag -d v2.1.0
```

고칠 것을 PR로 `main`에 합친 뒤 3번을 다시 합니다. 공개한 태그는 옮기지 않습니다.
