# 개발 문서

사용 방법은 [README](../README.md)를 보세요. 이 문서는 빌드와 릴리스를 다룹니다.

## 구조

Rust 하나로 두 플랫폼을 만듭니다.

| 경로 | 내용 |
|---|---|
| `src/` | 공통: 로컬 HTTPS 서버(`server.rs`), 빈 광고(`banner.rs`), 인증서(`certs.rs`), hosts(`hosts.rs`), 설정 단계(`setup.rs`), 업데이트(`feed.rs`, `updater.rs`), 트레이·메뉴 막대(`ui.rs`), 사용자에게 보이는 모든 문구(`notices.rs`) |
| `src/platform/` | `windows.rs`(설치·레지스트리·UAC·인증서 저장소), `macos.rs`(앱 번들·키체인·LaunchAgent) |
| `packaging/` | `windows/build.ps1`, `macos/build.sh`(앱 번들·서명·dmg), `macos/Info.plist` |
| `tools/` | 업데이트 서명 키, 릴리스 서명, macOS 코드 서명 인증서 |
| `docs/update-feed.md` | 업데이트 정보 형식과 서명 |
| `VERSION` | 두 플랫폼 공통 버전 (`Cargo.toml`의 `version`과 같게) |

### 빈 광고 (`banner.rs`)

- 카카오톡은 `serv.ds.kakao.com/sdk/banner`에 광고 목록(JSON)을 요청하고, 첫 광고의 HTML을 하단 웹뷰에 그립니다.
- 빈 목록을 주면 카카오톡이 내장 배너로 바꿔 버리므로, 배경색만 칠한 "광고" 하나를 돌려줍니다.
- Windows 카카오톡은 더 깐깐합니다.
  - 실제 광고와 같은 필드(이벤트, 피드백 링크, ID 등)가 있어야 받아들입니다. 링크는 모두 로컬 서버(`/ev/*`, 204)를 가리킵니다.
  - WebView2로 `ReadyToListen`을 보내고 `AdLoaded {result:true}` 응답을 기다립니다. 없으면 실패로 보고 대체 배너를 띄웁니다.
- 사이드바 부분의 폭은 페이지가 직접 계산합니다: `광고 칸 높이 × SIDEBAR_PER_SLOT`(Windows 122/188, macOS 78/100). Windows 웹뷰의 CSS 픽셀은 카카오톡 배율을 따라가지 않지만, 사이드바와 광고 칸은 함께 커지기 때문입니다.
- Windows는 사이드바 부분에 메뉴 버튼을 그립니다. 누르면 `/kakaoadhide/menu?t=<토큰>`을 요청하고, 앱이 커서 위치에 메뉴를 띄웁니다. 토큰은 처음 한 번 만들어 레지스트리(`MenuToken`)에 저장합니다.
- 테스트가 빈 광고에 외부 요청이 없는지 확인합니다(메뉴 버튼의 로컬 요청 하나만 허용).
- 광고 자리는 요청의 `id`로 구분합니다(`server.log`에 `[DAN-…]`로 남음). Windows는 하단 배너(`BOTTOM_BANNER_UNITS`)에만 빈 광고를 주고, 다른 자리(하루 첫 실행 때 트레이 근처에 뜨는 팝업 등)에는 빈 목록을 줘서 뜨지 않게 합니다. 카카오톡 업데이트로 하단 배너 `id`가 바뀌면 내장 배너가 다시 보이니, 로그를 보고 목록에 추가합니다. macOS는 아직 모든 자리에 빈 광고를 줍니다.

## 빌드

```bash
cargo test                       # 공통 테스트
packaging/macos/build.sh --dmg   # macOS: target/KakaoAdHide.app, dist/KakaoAdHide-macos.dmg
```

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1   # Windows: dist\KakaoAdHide-windows.exe
```

macOS에서 그냥 빌드하면 ad-hoc 서명이라 릴리스 앱과 서로 업데이트할 수 없습니다. 릴리스 인증서로 서명하려면 `--sign-p12 ~/.keys/kakaoadhide-macos-signing.p12 --sign-password-file ~/.keys/kakaoadhide-macos-signing.password`를 붙입니다.

## 릴리스

### 처음 한 번: 업데이트 서명 키

```powershell
powershell -ExecutionPolicy Bypass -File tools\new-signing-keys.ps1 -SetSecret
```

- RSA-3072 키 두 쌍(주 키, 예비 키)을 저장소 밖(`%USERPROFILE%\.keys`)에 만듭니다. 공개키는 `src/keys.rs`에 들어갑니다.
- `-SetSecret`을 붙이면 주 키의 개인키를 `SIGNING_PRIVATE_KEY` 시크릿으로 저장합니다(`gh` 로그인 필요).
- **개인키 두 개를 반드시 백업하세요**(비밀번호 관리자, 암호화한 USB 등). GitHub 시크릿은 다시 꺼내 볼 수 없습니다.
  예비 키는 릴리스에 쓰지 않으니 백업한 뒤 PC에서 지워 두세요. 주 키를 잃어버리면 예비 키로 서명한 업데이트로 키를 바꿀 수 있습니다.
- 바뀐 `src/keys.rs`(공개키만 들어 있음)를 커밋합니다.

### 처음 한 번: macOS 코드 서명 인증서

```bash
tools/new-macos-signing-cert.sh
tools/new-macos-signing-cert.sh --set-secret
```

- 자체 서명 인증서(무료)를 저장소 밖(`~/.keys`)에 만들고 SHA-1을 출력합니다. 그 값이 `src/platform/macos.rs`의 `SIGNING_CERT_SHA1`에 들어갑니다.
- `--set-secret`은 `MACOS_SIGNING_P12`, `MACOS_SIGNING_PASSWORD` 시크릿을 저장합니다(`gh` 로그인 필요).
- **두 파일을 반드시 백업하세요.** 인증서를 바꾸면 설치된 앱이 업데이트를 거부합니다.

### 버전마다

버전 규칙: 버그 수정은 셋째 자리(2.0.**1**), 새 기능은 둘째 자리(2.**1**.0), 방식이 크게 바뀌면 첫째 자리(**3**.0.0).

1. 버전을 올립니다.
   - `VERSION` 파일과 `Cargo.toml`의 `version`을 새 버전으로 바꿉니다.
   - `Cargo.lock`도 맞춥니다. 빠뜨리면 CI(`--locked`)가 실패합니다.
     ```powershell
     cargo update -p kakaoadhide
     ```
2. 커밋하고 푸시합니다.
   ```powershell
   git add -A; git commit -m "..."; git push origin main
   ```
3. 같은 이름의 태그를 올립니다. 태그 이름(`v2.1.0`)과 `VERSION`(`2.1.0`)이 다르면 CI가 멈춥니다.
   ```powershell
   git tag -a v2.1.0 -m v2.1.0; git push origin v2.1.0
   ```
4. GitHub Actions가 두 플랫폼 빌드 → 업데이트 정보 서명 → 각 플랫폼 앱으로 서명 검증 → **초안(draft) 릴리스** 생성까지 합니다.
   올라가는 파일: `KakaoAdHide-windows.exe`, `latest-windows.json`, `KakaoAdHide-macos.dmg`, `latest-macos.json`, `SHA256SUMS.txt`
5. 초안에 릴리스 노트를 쓰고 **Publish release**를 누르면, 그때부터 사용자에게 업데이트 알림이 갑니다.

공개 전에 태그를 다시 붙여야 하면(CI 실패를 고친 뒤 등):

```powershell
git push origin :refs/tags/v2.1.0; git tag -d v2.1.0
```

그다음 3번을 다시 합니다. 공개한 태그는 옮기지 않습니다.
