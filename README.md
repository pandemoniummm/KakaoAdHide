# KakaoAdHide

카카오톡 PC(Windows, macOS)의 광고 배너를 비우는 작은 프로그램입니다.
카카오톡이 광고를 받아 오는 요청을 이 컴퓨터 안에서 대신 받아, 광고 대신 **카카오톡 배경색만 칠한 빈 칸**을 돌려줍니다.

> 비공식 프로젝트입니다. 카카오(Kakao Corp.)와 관련이 없습니다.

## 특징

- **카카오톡이 직접 그리는 빈 칸**: 광고 칸 자리에 카카오톡 배경색(사이드바·목록)만 그려집니다. 카카오톡 창 안에서 카카오톡이 그리기 때문에, 창을 끌거나 크기를 바꿔도 어긋나거나 광고가 비치지 않습니다.
- **카카오톡을 건드리지 않음**: 실행 파일 수정이나 코드 주입이 없습니다. 광고 주소 두 개만 이 컴퓨터로 보내고, 그 요청에 빈 광고를 돌려줍니다.
- **광고 서버에 접속하지 않음**: 광고 노출·클릭 기록도 보내지 않습니다.
- **가벼움**: 카카오톡이 한 시간에 몇 번 보내는 요청에만 답합니다.
- **안전한 업데이트**: 새 버전이 나오면 알려 주고, 업데이트를 고르면 서명을 확인한 뒤에만 설치합니다.

## 동작 방식과 보안

설치하고 처음 실행할 때, 동의를 받은 뒤 한 번만 설정합니다.

1. **인증서**: 이 컴퓨터에서 새로 만든 인증서를 신뢰하도록 등록합니다. 이 인증서는
   - 카카오톡 광고 주소 두 개(`serv.ds.kakao.com`, `display.ad.daum.net`)에만 쓸 수 있고,
   - 다른 인증서를 발급할 수 없는 종류(CA 아님)이며,
   - 개인키가 이 컴퓨터를 떠나지 않습니다(현재 사용자만 읽을 수 있는 파일).

   그래서 개인키가 새더라도 다른 사이트(은행, 메일 등)를 위조할 수는 없고, 카카오톡 광고 칸에 다른 내용을 띄우는 정도만 가능합니다.
2. **hosts 파일**: 두 광고 주소를 이 컴퓨터(127.0.0.1)로 보내는 줄을 넣습니다. 관리자 권한이 필요합니다(Windows: UAC 확인, macOS: 관리자 암호).
3. **로컬 서버**: KakaoAdHide가 443 포트에서 **이 컴퓨터에서 온 연결만** 받아, 광고 요청에는 빈 광고를, 그 밖의 요청에는 빈 응답을 돌려줍니다.
   빈 광고에는 Windows 카카오톡에 '광고 로드됨'을 알리는 짧은 스크립트만 있고, 외부 요청은 없습니다(테스트로 확인).

알아 두실 점:
- 같은 광고 주소를 쓰는 **웹사이트(다음 등)의 광고도 함께 안 보일 수 있습니다.**
- KakaoAdHide가 꺼져 있으면 광고 요청이 실패해서 카카오톡 내장 배너가 뜰 뿐, 다른 인터넷 통신에는 영향이 없습니다.
- 메뉴의 **제거**로 hosts 줄, 인증서, 설정을 모두 원래대로 되돌립니다.
- 10분마다 광고 주소가 이 컴퓨터로 오는지 확인하고, 아니면 메뉴에 표시합니다(**설정하기**로 다시 적용).
- 카카오톡이 광고 주소나 응답 형식을 바꾸면 내장 배너가 다시 보일 수 있습니다. 그럴 때는 카카오톡 버전과 함께 이슈로 알려 주세요.

# Windows

## 설치

1. [Releases](../../releases)에서 `KakaoAdHide-windows.exe`를 받아 실행합니다. 설치가 끝나면 KakaoAdHide가 카카오톡을 켭니다.
2. 처음 실행하면 설정 안내가 뜹니다. **설정하기** → 인증서 등록 확인 창에서 **예** → UAC 확인 창에서 **예**.

처음 실행할 때 **"Windows의 PC 보호"** 창이 뜨면 **추가 정보 → 실행**을 누르세요. 코드 서명을 하지 않은 프로그램이라 뜨는 경고입니다.
받은 파일이 Releases의 파일과 같은지는 함께 올린 `SHA256SUMS.txt`로 확인할 수 있습니다.

```powershell
Get-FileHash .\KakaoAdHide-windows.exe -Algorithm SHA256
```

### 설치하면 바뀌는 것

| 항목 | 내용 |
|---|---|
| `%LOCALAPPDATA%\KakaoAdHide\KakaoAdHide.exe` | 프로그램 파일 (인증서도 이 폴더에) |
| 시작 프로그램 폴더의 `KakaoAdHide.lnk` | 로그인할 때 `--run`으로 실행 |
| 카카오톡 자체 자동 실행 (`HKCU\...\Run\KakaoTalk`) | 꺼 둡니다. KakaoAdHide가 서버를 먼저 띄운 뒤 카카오톡을 트레이로 켭니다. 원래 값은 저장해 두었다가 제거할 때 돌려놓습니다. |
| 현재 사용자의 신뢰할 수 있는 루트 인증서 | `KakaoAdHide local ad server` (광고 주소 두 개 전용) |
| `C:\Windows\System32\drivers\etc\hosts` | `# KakaoAdHide begin` ~ `# KakaoAdHide end` 사이의 두 줄 |
| 설정 > 앱 > 설치된 앱 | `KakaoAdHide` 항목 |

## 제거

**설정 > 앱 > 설치된 앱 > KakaoAdHide > 제거**, 또는 트레이 아이콘 메뉴의 **KakaoAdHide 제거…**

```powershell
& "$env:LOCALAPPDATA\KakaoAdHide\KakaoAdHide.exe" --uninstall
```

## 명령줄

| 명령 | 동작 |
|---|---|
| `KakaoAdHide-windows.exe` (받은 파일) | 설치 후 실행 |
| `KakaoAdHide.exe` (설치된 파일) | 바로 실행 |
| `KakaoAdHide.exe --run` | 로그인할 때 쓰는 모드(카카오톡을 트레이로 시작) |
| `KakaoAdHide.exe --uninstall` | 제거 |
| `KakaoAdHide.exe --update` | 지금 업데이트 확인(새 버전이 있으면 검증 후 설치) |
| `--quiet` | 설치, 제거할 때 안내 창을 띄우지 않음 |

KakaoAdHide 1.0.0(창을 숨기던 버전)에서 업데이트하면 그대로 이 방식으로 바뀌고, 처음 실행할 때 설정 안내가 뜹니다.

# macOS

## 설치

1. [Releases](../../releases)에서 `KakaoAdHide-macos.dmg`를 받아 열고, `KakaoAdHide`를 `Applications`로 끌어다 놓습니다.
2. 응용 프로그램 폴더에서 KakaoAdHide를 실행합니다.
   - **"Apple은 'KakaoAdHide'에 악성 코드가 없음을 확인할 수 없습니다"** 창이 뜨면 **완료**를 누르고, **시스템 설정 > 개인정보 보호 및 보안** 아래쪽의 **그래도 열기**를 누른 뒤 다시 실행합니다. Apple 유료 개발자 서명(공증)을 하지 않은 앱이라 처음 한 번 뜨는 경고입니다.
3. 설정 안내에서 **설정하기** → 인증서 신뢰 변경에 **Mac 로그인 암호** → hosts 변경에 **관리자 암호**.
4. 메뉴 막대에 아이콘이 생깁니다. 메뉴에서 업데이트, 로그인 시 실행, 제거를 고를 수 있습니다.

macOS 11 이상, Apple silicon과 Intel을 지원합니다.

카카오톡이 KakaoAdHide보다 먼저 켜지면, KakaoAdHide가 켜진 뒤 카카오톡이 광고를 다시 요청할 때(최대 1분)까지 내장 배너가 보일 수 있습니다.

### 설치하면 바뀌는 것

| 항목 | 내용 |
|---|---|
| `/Applications/KakaoAdHide.app` | 프로그램 (쓸 수 없으면 `~/Applications`) |
| `~/Library/LaunchAgents/io.github.pandemoniummm.KakaoAdHide.plist` | 로그인할 때 실행 (메뉴에서 끌 수 있음) |
| 로그인 키체인 | `KakaoAdHide local ad server` 인증서 (SSL에만 신뢰, 광고 주소 두 개 전용) |
| `/etc/hosts` | `# KakaoAdHide begin` ~ `# KakaoAdHide end` 사이의 두 줄 |
| `~/Library/Application Support/KakaoAdHide/` | 설정과 인증서 |

## 제거

메뉴 막대 아이콘 > **KakaoAdHide 제거…** (hosts 줄, 인증서, 로그인 항목, 설정을 지우고 앱을 휴지통으로 옮깁니다)

# 업데이트 (두 플랫폼 공통)

- 실행 1분 뒤, 그 뒤로 하루 한 번 GitHub의 최신 릴리스(`releases/latest/download/latest-windows.json`, `latest-macos.json`)를 확인합니다. 요청에는 `User-Agent: KakaoAdHide/<버전>` 말고 아무 정보도 담지 않습니다. GitHub에는 접속 IP가 남습니다.
- 새 버전이 있으면 알려 주고 아이콘에 표시합니다. **업데이트를 고르기 전에는 아무것도 받지 않습니다.**
- 받은 파일은 아래를 모두 확인한 뒤에만 설치합니다. 하나라도 맞지 않으면 설치하지 않고 릴리스 페이지를 안내합니다.
  - 다운로드 주소가 이 저장소의 Releases인지
  - 파일 크기와 SHA-256이 업데이트 정보와 같은지
  - 업데이트 정보의 서명(버전 + SHA-256)이 프로그램에 들어 있는 공개키와 맞는지
  - (macOS) dmg 안 앱의 번들 ID·버전과 코드 서명
- 형식: [docs/update-feed.md](docs/update-feed.md)
- 업데이트 확인 끄기: 메뉴의 **업데이트 자동 확인**, 또는 Windows 레지스트리 `HKCU\Software\KakaoAdHide`에 DWORD 값 `UpdateCheck` = `0`

# 빌드

Rust 하나로 두 플랫폼을 만듭니다.

| 경로 | 내용 |
|---|---|
| `src/` | 공통: 로컬 HTTPS 서버(`server.rs`), 빈 광고(`banner.rs`), 인증서(`certs.rs`), hosts(`hosts.rs`), 설정 단계(`setup.rs`), 업데이트(`feed.rs`, `updater.rs`), 트레이·메뉴 막대(`ui.rs`) |
| `src/platform/` | `windows.rs`(설치·레지스트리·UAC·인증서 저장소), `macos.rs`(앱 번들·키체인·LaunchAgent) |
| `packaging/` | `windows/build.ps1`, `macos/build.sh`(앱 번들·서명·dmg), `macos/Info.plist` |
| `tools/` | 업데이트 서명 키, 릴리스 서명, macOS 코드 서명 인증서 |
| `docs/update-feed.md` | 업데이트 정보 형식과 서명 |
| `VERSION` | 두 플랫폼 공통 버전 |

```bash
cargo test                       # 공통 테스트
packaging/macos/build.sh --dmg   # macOS: target/KakaoAdHide.app, dist/KakaoAdHide-macos.dmg
```

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1   # Windows: dist\KakaoAdHide-windows.exe
```

macOS에서 그냥 빌드하면 ad-hoc 서명이라 릴리스 앱과 서로 업데이트할 수 없습니다. 릴리스 인증서로 서명하려면 `--sign-p12 ~/.keys/kakaoadhide-macos-signing.p12 --sign-password-file ~/.keys/kakaoadhide-macos-signing.password`를 붙입니다.

# 릴리스 (관리자용)

## 처음 한 번: 업데이트 서명 키 만들기

```powershell
powershell -ExecutionPolicy Bypass -File tools\new-signing-keys.ps1 -SetSecret
```

- RSA-3072 키 두 쌍(주 키, 예비 키)을 저장소 밖(`%USERPROFILE%\.keys`)에 만듭니다. 공개키는 `src/keys.rs`에 들어갑니다.
- `-SetSecret`을 붙이면 주 키의 개인키를 이 저장소의 `SIGNING_PRIVATE_KEY` 시크릿으로 저장합니다(`gh` 로그인 필요).
- **개인키 두 개를 반드시 백업하세요**(비밀번호 관리자, 암호화한 USB 등). GitHub 시크릿은 다시 꺼내 볼 수 없습니다.
  예비 키는 릴리스에 쓰지 않으니 백업한 뒤 PC에서 지워 두세요. 주 키를 잃어버리면 예비 키로 서명한 업데이트로 키를 바꿀 수 있습니다.
- 바뀐 `src/keys.rs`(공개키만 들어 있음)를 커밋합니다.

## 처음 한 번: macOS 코드 서명 인증서

```bash
tools/new-macos-signing-cert.sh
tools/new-macos-signing-cert.sh --set-secret
```

- 자체 서명 인증서(무료)를 저장소 밖(`~/.keys`)에 만들고 SHA-1을 출력합니다. 그 값이 `src/platform/macos.rs`의 `SIGNING_CERT_SHA1`에 들어갑니다.
- `--set-secret`은 `MACOS_SIGNING_P12`, `MACOS_SIGNING_PASSWORD` 시크릿을 저장합니다(`gh` 로그인 필요).
- **두 파일을 반드시 백업하세요.** 인증서를 바꾸면 설치된 앱이 업데이트를 거부합니다.

## 버전마다

1. `VERSION`을 올리고 커밋합니다(예: `1.1.0`).
2. 같은 이름의 태그를 푸시합니다: `git tag v1.1.0; git push origin v1.1.0`
3. GitHub Actions가 두 플랫폼 빌드 → 업데이트 정보 서명 → 각 플랫폼 앱으로 서명 검증 → **초안(draft) 릴리스** 생성까지 합니다.
   올라가는 파일: `KakaoAdHide-windows.exe`, `latest-windows.json`, `KakaoAdHide-macos.dmg`, `latest-macos.json`, `SHA256SUMS.txt`(두 플랫폼 공통)
4. 초안을 확인하고 **Publish**를 누르면 그때부터 사용자에게 업데이트 알림이 갑니다.

# 확인한 환경

- macOS 27.2 (Apple silicon), 카카오톡 Mac 26.10.0
- Windows 11, 카카오톡 PC 26.10.0 (광고 칸 동작은 아직 확인 중)

# 면책

이 프로그램은 있는 그대로 제공되며, 사용에 따른 책임은 사용자에게 있습니다. 자세한 내용은 [LICENSE](LICENSE)를 보세요.
