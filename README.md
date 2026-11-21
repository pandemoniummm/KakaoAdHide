# KakaoAdHide

카카오톡 PC의 광고 배너를 숨기는 작은 프로그램입니다. Windows와 macOS를 지원합니다.

- **Windows**: `KakaoAdHide-windows.exe`를 한 번 실행하면 설치가 끝나고, 이후 로그인할 때마다 카카오톡을 대신 켜서 광고를 숨깁니다. 광고 자리까지 없애고 목록을 늘립니다.
- **macOS**: `KakaoAdHide-macos.dmg`의 앱을 응용 프로그램 폴더에 넣고 실행합니다. 광고 위를 카카오톡 배경색으로 덮습니다(아래 [macOS](#macos) 참고).

> 비공식 프로젝트입니다. 카카오(Kakao Corp.)와 관련이 없습니다.

# Windows

## 특징

- **광고 자리까지 없앰**: 광고 창을 숨기고, 친구/채팅 목록을 창 바닥까지 늘려서 처음부터 광고가 없는 것처럼 보입니다.
- **시작할 때도 안 보임**: 카카오톡이 켜지면서 잠깐 그리는 기본 배너까지 가립니다.
- **창 조절이 느려지지 않음**: 창을 끌거나 크기를 바꿀 때 카카오톡을 방해하지 않습니다.
- **카카오톡을 건드리지 않음**: 실행 파일 수정, 코드 주입, 통신 차단이 없습니다. 화면에 보이는 창만 일반 Windows API로 숨깁니다.
- **가벼움**: 창이 바뀔 때만 동작합니다(메모리 약 25MB). 관리자 권한이 필요 없습니다.
- **안전한 업데이트**: 새 버전이 나오면 알림을 띄우고, 클릭하면 서명을 확인한 뒤에만 업데이트합니다.

## 설치

1. [Releases](../../releases)에서 `KakaoAdHide-windows.exe`를 받습니다.
2. 실행합니다. 설치가 끝나면 카카오톡이 광고 없이 켜집니다. 받은 파일은 설치 후 지워도 됩니다(`KakaoAdHide.exe`로 설치됨).

처음 실행할 때 **"Windows의 PC 보호"** 창이 뜨면 **추가 정보 → 실행**을 누르세요.
코드 서명을 하지 않은 프로그램이라 뜨는 경고입니다. 받은 파일이 Releases의 파일과 같은지는 함께 올린 `SHA256SUMS.txt`로 확인할 수 있습니다.

```powershell
Get-FileHash .\KakaoAdHide-windows.exe -Algorithm SHA256
```

### 설치하면 바뀌는 것

모두 현재 사용자 범위이고, 제거하면 원래대로 돌아갑니다.

| 항목 | 내용 |
|---|---|
| `%LOCALAPPDATA%\KakaoAdHide\KakaoAdHide.exe` | 프로그램 파일 |
| 시작 프로그램 폴더의 `KakaoAdHide.lnk` | 로그인할 때 `--run`으로 실행 |
| 카카오톡 자체 자동 실행 (`HKCU\...\Run\KakaoTalk`) | 꺼 둡니다. KakaoAdHide가 대신 카카오톡을 트레이로 켭니다. 원래 값은 저장해 두었다가 제거할 때 돌려놓습니다. |
| 설정 > 앱 > 설치된 앱 | `KakaoAdHide` 항목 |

카카오톡 자동 실행을 끄는 이유: 카카오톡이 먼저 켜지면 광고가 잠깐 보일 수 있어서, KakaoAdHide가 감시를 시작한 뒤 카카오톡을 켭니다.
카카오톡 설정에서 자동 실행을 다시 켜거나 업데이트로 다시 등록돼도, KakaoAdHide가 실행될 때 다시 끕니다.

## 업데이트

- 실행 1분 뒤, 그 뒤로 하루 한 번 GitHub의 최신 릴리스(`releases/latest/download/latest-windows.json`)를 확인합니다. 인터넷 접속은 이것 하나뿐이고, 요청에는 `User-Agent: KakaoAdHide/<버전>` 말고 아무 정보도 담지 않습니다. GitHub에는 접속 IP가 남습니다.
- 새 버전이 있으면 트레이 알림이 뜹니다. **클릭하기 전에는 아무것도 받지 않습니다.**
  트레이 아이콘을 오른쪽 클릭하면 업데이트, 변경 내역 보기, 이 버전 건너뛰기, 나중에를 고를 수 있습니다.
- 클릭하면 새 exe를 받아서 아래를 모두 확인한 뒤에만 설치합니다. 하나라도 맞지 않으면 설치하지 않고 릴리스 페이지를 안내합니다.
  - 다운로드 주소가 이 저장소의 Releases인지
  - 파일 크기와 SHA-256이 `latest-windows.json`과 같은지
  - `latest-windows.json`의 서명(버전 + SHA-256)이 프로그램에 들어 있는 공개키와 맞는지
- 지금 바로 확인: `KakaoAdHide.exe --update`
- 업데이트 확인 끄기: 레지스트리 `HKCU\Software\KakaoAdHide`에 DWORD 값 `UpdateCheck` = `0`

## 제거

**설정 > 앱 > 설치된 앱 > KakaoAdHide > 제거**

또는:

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
| `--quiet` | 설치, 제거, 업데이트할 때 안내 창을 띄우지 않음 |

## 동작 원리

카카오톡 창의 변화를 `SetWinEventHook`(out-of-context)으로 받아서 다음을 처리합니다.

1. **광고 팝업**: 메인 창이 소유한 제목 없는 `EVA_Window_Dblclk` 창(`Chrome_WidgetWin_0` 포함, 높이 200 이하)을 만들어지는 순간 찾습니다. 그 창을 투명(alpha 0)하게, 클릭이 통과되게, 빈 영역으로 만들고 숨깁니다.
   AdFit 광고 뷰(`AdFit NAS Advertisement`)는 WebView2 프로세스에서 나중에 붙기 때문에, 그걸 기다리지 않고 창 모양으로 먼저 알아봅니다.
2. **빈 자리**: 메인 창 안의 제목 없는 `EVA_ChildWindow`(높이 80~200)를 숨깁니다.
3. **목록 늘리기**: `OnlineMainView`를 창 바닥까지 늘립니다. 창을 끄는 동안에는 쉬고, 마우스를 놓을 때 한 번에 늘립니다.
4. **시작 시 가림막**: 카카오톡은 시작할 때 광고 팝업을 만들고 그리는 동안(약 150~200ms) 다른 프로세스의 요청을 처리하지 않습니다.
   그동안 광고 위에 클릭이 통과되는 가림막을 잠깐 띄웁니다. 가림막 색은 바로 위 화면 색을 따서 칠합니다.

카카오톡을 기다리는 호출(`SetWindowRgn`, `SetWindowLong`, `SetWindowPos`)은 모두 별도 스레드에서 실행해서, 카카오톡이 바쁠 때도 감시가 밀리지 않습니다.

# macOS

## 특징

- **광고를 덮음**: 친구/채팅 목록 창 바닥의 광고(높이 100pt) 위에 카카오톡 배경색 가림막을 올립니다. 광고는 보이지도 눌리지도 않습니다.
- **Windows판과 다른 점**: macOS에는 다른 앱 창 안의 일부를 숨기거나 크기를 바꾸는 방법이 없어서, **광고 자리(빈 공간)는 남고 목록을 늘릴 수 없습니다.** 광고 서버를 막아도 카카오톡이 내장 배너로 같은 자리를 채워서, 덮는 방법이 유일합니다.
- **창을 따라감**: 창 이동·크기 조절 중에도 화면 갱신마다 위치를 맞춥니다. 다른 앱 창이 위에 오면 가림막도 함께 가려집니다.
- **카카오톡을 건드리지 않음**: 실행 파일 수정, 코드 주입, 통신 차단이 없습니다. 손쉬운 사용 API로 창 위치와 광고 영역의 위치만 읽고, 대화 내용은 읽거나 저장하지 않습니다.
- **안전한 업데이트**: Windows판과 같은 서명 방식에 더해, dmg 안 앱의 코드 서명까지 확인한 뒤에만 업데이트합니다.

## 설치

1. [Releases](../../releases)에서 `KakaoAdHide-macos.dmg`를 받아 열고, `KakaoAdHide`를 `Applications`로 끌어다 놓습니다.
2. 응용 프로그램 폴더에서 KakaoAdHide를 실행합니다.
   - **"Apple은 'KakaoAdHide'에 악성 코드가 없음을 확인할 수 없습니다"** 창이 뜨면 **완료**를 누르고, **시스템 설정 > 개인정보 보호 및 보안** 아래쪽의 **그래도 열기**를 누른 뒤 다시 실행합니다. Apple 유료 개발자 서명(공증)을 하지 않은 앱이라 처음 한 번 뜨는 경고입니다.
3. **손쉬운 사용 권한**을 요청하면 **설정 열기**를 눌러 목록의 KakaoAdHide를 켭니다. 켜는 즉시 동작합니다.
4. 메뉴 막대에 눈 아이콘이 생깁니다. 이 아이콘 메뉴에서 켜기/끄기, 가림막 색, 업데이트, 로그인 시 실행, 제거를 고를 수 있습니다.

받은 파일이 Releases의 파일과 같은지는 `SHA256SUMS.txt`로 확인할 수 있습니다.

```bash
shasum -a 256 KakaoAdHide-macos.dmg
```

macOS 13 이상, Apple silicon과 Intel을 지원합니다.

### 설치하면 바뀌는 것

| 항목 | 내용 |
|---|---|
| `/Applications/KakaoAdHide.app` | 프로그램 (쓸 수 없으면 `~/Applications`) |
| 시스템 설정 > 일반 > 로그인 항목 | KakaoAdHide (메뉴에서 끌 수 있음) |
| 시스템 설정 > 개인정보 보호 및 보안 > 손쉬운 사용 | KakaoAdHide (사용자가 직접 켬) |
| `~/Library/Preferences/io.github.pandemoniummm.KakaoAdHide.plist` | 설정 |

카카오톡 자체의 자동 실행은 건드리지 않습니다. KakaoAdHide가 카카오톡이 켜지는 것을 감지해서 따라붙습니다.

## 업데이트

- Windows판과 같습니다. 실행 1분 뒤, 그 뒤로 하루 한 번 `releases/latest/download/latest-macos.json`을 확인하고, 새 버전이 있으면 알림을 띄우고 메뉴 막대 아이콘이 바뀝니다. **클릭하기 전에는 아무것도 받지 않습니다.**
- 업데이트를 누르면 dmg를 받아서 아래를 모두 확인한 뒤에만 바꿉니다. 하나라도 맞지 않으면 설치하지 않고 릴리스 페이지를 안내합니다.
  - 다운로드 주소, 파일 크기, SHA-256, `latest-macos.json`의 서명(Windows판과 같은 공개키)
  - dmg 안 앱의 번들 ID와 버전, 그리고 릴리스 인증서로 서명됐는지(코드 서명)
- 자세한 형식: [docs/update-feed.md](docs/update-feed.md)
- 업데이트 확인 끄기: 메뉴의 **업데이트 자동 확인**, 또는 `defaults write io.github.pandemoniummm.KakaoAdHide UpdateCheck -bool false`

## 제거

메뉴 막대 아이콘 > **KakaoAdHide 제거…** (로그인 항목과 설정을 지우고 앱을 휴지통으로 옮깁니다). 손쉬운 사용 목록에 남아 있으면 `-` 버튼으로 지워 주세요.

## 동작 원리

1. 손쉬운 사용 API로 카카오톡의 `Main Window` 안에서 **웹 뷰(`AXWebArea`)를 담은, 창 바닥에 붙은 전체 폭 그룹**을 찾습니다. 이 모양이 아니면(카카오톡이 구조를 바꾸면) 엉뚱한 곳을 덮지 않도록 가림막을 띄우지 않습니다.
2. 그 위치에 테두리 없는 창(포커스를 가져가지 않음)을 띄웁니다. 창 모양을 광고 크기로 잘라 아래 두 모서리를 카톡 창처럼 둥글게 하고, 왼쪽 사이드바 폭은 사이드바 색, 나머지는 목록 색으로 칠합니다(라이트/다크). 광고 가장자리 5pt는 클릭이 카톡으로 통과해서 카톡 창 크기 조절 손잡이를 그대로 쓸 수 있습니다.
3. 광고 자리 위에 다른 창이 없으면 가림막을 한 층 위(floating)에 둡니다. 겉보기는 같고, 카톡을 누를 때 창이 앞으로 올라와도 광고가 깜빡 보이지 않습니다. 다른 창(카톡 채팅창 포함)이 광고 자리에 겹치면 일반 층으로 내려 그 창 아래, 카톡 바로 위에 둡니다.
4. 창 이동·크기·최소화·활성화·숨김을 `AXObserver`로 받고, 0.5초마다 한 번 더 확인합니다.
5. 카톡 창을 끄는 동안은 가림막을 창 서버에 직접 옮기고 모양을 바꿉니다. 손쉬운 사용 알림은 끄는 동안 초당 9번 정도만 오고, AppKit은 창 크기를 화면 갱신 주기에 맞춰 늦게 반영하기 때문입니다.
   - **마우스**: 이벤트 탭으로 마우스 이벤트를 macOS보다 먼저 받아, 카톡 창이 옮겨질 위치로 바로 옮깁니다.
   - **트랙패드**: 별도 스레드가 약 1ms마다 창 서버에서 카톡 창 위치를 읽어 따라갑니다. 트랙패드는 macOS가 창을 다르게 움직여서, 실제 입력으로 화면을 재 보면 이쪽이 더 정확했습니다.
   - 다른 앱 창을 같은 창 서버 그룹으로 묶을 수는 없어서(Dock만 가능), 아주 빠르게 흔들면 가장자리가 잠깐 비치는 순간은 남습니다.
6. 화면 기록 권한은 필요 없습니다(창 위치만 읽고 화면 내용은 읽지 않음).

비공개 API를 씁니다. 모두 window manager 앱들이 오래 써 온 것이고, 없어지면 기능을 줄여서 동작합니다.
- `_AXUIElementGetWindow`: 손쉬운 사용 창 → 창 번호 (없으면 가림막을 띄우지 않음)
- `SLSMoveWindow`, `SLSSetWindowShape`, `SLSSetWindowEventShape`: **우리 가림막 창만** 옮기고 모양을 바꿈 (없으면 AppKit으로 옮김. 느리지만 동작)

이벤트 탭은 마우스를 누르고 있는 동안에만 만들고, 이벤트를 읽기만 하고 그대로 돌려보냅니다(손쉬운 사용 권한으로 동작).

# 빌드

## Windows

Windows에 기본으로 들어 있는 .NET Framework 4.x 컴파일러를 씁니다. 따로 설치할 것은 없습니다.

```powershell
powershell -ExecutionPolicy Bypass -File windows\build.ps1
```

결과물은 `dist\KakaoAdHide-windows.exe`와 `dist\SHA256SUMS.txt`입니다. 버전은 `VERSION` 파일에서 읽습니다.

## macOS

Xcode Command Line Tools만 있으면 됩니다(`xcode-select --install`).

```bash
macos/build.sh --dmg
```

결과물은 `macos/build/KakaoAdHide.app`과 `dist/KakaoAdHide-macos.dmg`입니다. 테스트는 `cd macos && swift test`.
그냥 빌드하면 ad-hoc 서명이라 빌드할 때마다 손쉬운 사용 권한을 다시 켜야 합니다. 릴리스 인증서로 서명하려면 `--sign-p12 ~/.keys/kakaoadhide-macos-signing.p12 --sign-password-file ~/.keys/kakaoadhide-macos-signing.password`를 붙입니다.

## 저장소 구조

| 경로 | 내용 |
|---|---|
| `windows/` | Windows판 (C#, `build.ps1`) |
| `macos/` | macOS판 (Swift Package, `build.sh`) |
| `tools/` | 두 플랫폼 공통: 업데이트 서명 키, 릴리스 서명, macOS 코드 서명 인증서 |
| `docs/update-feed.md` | 업데이트 정보 형식과 서명 (두 플랫폼 공통) |
| `VERSION` | 두 플랫폼 공통 버전 |

# 릴리스 (관리자용)

## 처음 한 번: 업데이트 서명 키 만들기

```powershell
powershell -ExecutionPolicy Bypass -File tools\new-signing-keys.ps1 -SetSecret
```

- RSA-3072 키 두 쌍(주 키, 예비 키)을 저장소 밖(`%USERPROFILE%\.keys`)에 만듭니다. 공개키는 `windows\src\UpdateKeys.cs`와 `macos/Sources/Core/UpdateKeys.swift`에 들어갑니다.
- `-SetSecret`을 붙이면 주 키의 개인키를 이 저장소의 `SIGNING_PRIVATE_KEY` 시크릿으로 저장합니다(`gh` 로그인 필요).
- **개인키 두 개를 반드시 백업하세요**(비밀번호 관리자, 암호화한 USB 등). GitHub 시크릿은 다시 꺼내 볼 수 없습니다.
  예비 키는 릴리스에 쓰지 않으니 백업한 뒤 PC에서 지워 두세요. 주 키를 잃어버리면 예비 키로 서명한 업데이트로 키를 바꿀 수 있습니다.
- 바뀐 두 `UpdateKeys` 파일(공개키만 들어 있음)을 커밋합니다.
- 다른 저장소에서 같은 키를 쓰려면 그 저장소에서 `-UseExisting -SetSecret`으로 실행합니다.

## 처음 한 번: macOS 코드 서명 인증서

```bash
tools/new-macos-signing-cert.sh
tools/new-macos-signing-cert.sh --set-secret
```

- 자체 서명 인증서(무료)를 저장소 밖(`~/.keys`)에 만들고 SHA-1을 출력합니다. 그 값이 `macos/Sources/App/Config.swift`의 `signingCertificateSHA1`에 들어갑니다.
- `--set-secret`은 `MACOS_SIGNING_P12`, `MACOS_SIGNING_PASSWORD` 시크릿을 저장합니다(`gh` 로그인 필요).
- **두 파일을 반드시 백업하세요.** 인증서를 바꾸면 설치된 앱이 업데이트를 거부하고 손쉬운 사용 권한도 다시 켜야 합니다.

## 버전마다

1. `VERSION`을 올리고 커밋합니다(예: `1.0.1`).
2. 같은 이름의 태그를 푸시합니다: `git tag v1.0.1; git push origin v1.0.1`
3. GitHub Actions가 두 플랫폼 빌드 → 업데이트 정보 서명 → 각 플랫폼 앱으로 서명 검증 → **초안(draft) 릴리스** 생성까지 합니다.
   올라가는 파일: `KakaoAdHide-windows.exe`, `latest-windows.json`, `KakaoAdHide-macos.dmg`, `latest-macos.json`, `SHA256SUMS.txt`(두 플랫폼 공통)
4. 초안을 확인하고 **Publish**를 누르면 그때부터 사용자에게 업데이트 알림이 갑니다.

# 확인한 환경

- Windows 11 (24H2 이후), 카카오톡 PC 26.9.2
- macOS 27.2 (Apple silicon), 카카오톡 Mac 26.10.0

카카오톡이 업데이트로 창 구조를 바꾸면 동작하지 않을 수 있습니다. 광고가 다시 보이면 카카오톡 버전과 함께 이슈로 알려 주세요.

# 면책

이 프로그램은 있는 그대로 제공되며, 사용에 따른 책임은 사용자에게 있습니다. 자세한 내용은 [LICENSE](LICENSE)를 보세요.
