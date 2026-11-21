# KakaoAdHide

카카오톡 PC(Windows)의 광고 배너를 숨기는 작은 프로그램입니다.
`KakaoAdHide-windows.exe`를 한 번 실행하면 설치가 끝나고, 이후 로그인할 때마다 카카오톡을 대신 켜서 광고를 숨깁니다.

> 비공식 프로젝트입니다. 카카오(Kakao Corp.)와 관련이 없습니다.

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

## 빌드

Windows에 기본으로 들어 있는 .NET Framework 4.x 컴파일러를 씁니다. 따로 설치할 것은 없습니다.

```powershell
powershell -ExecutionPolicy Bypass -File build.ps1
```

결과물은 `dist\KakaoAdHide-windows.exe`와 `dist\SHA256SUMS.txt`입니다. 버전은 `VERSION` 파일에서 읽습니다.

## 릴리스 (관리자용)

### 처음 한 번: 서명 키 만들기

```powershell
powershell -ExecutionPolicy Bypass -File tools\new-signing-keys.ps1 -SetSecret
```

- RSA-3072 키 두 쌍(주 키, 예비 키)을 저장소 밖(`%USERPROFILE%\.keys`)에 만듭니다. 공개키는 `src\UpdateKeys.cs`에 들어갑니다.
- `-SetSecret`을 붙이면 주 키의 개인키를 이 저장소의 `SIGNING_PRIVATE_KEY` 시크릿으로 저장합니다(`gh` 로그인 필요).
- **개인키 두 개를 반드시 백업하세요**(비밀번호 관리자, 암호화한 USB 등). GitHub 시크릿은 다시 꺼내 볼 수 없습니다.
  예비 키는 릴리스에 쓰지 않으니 백업한 뒤 PC에서 지워 두세요. 주 키를 잃어버리면 예비 키로 서명한 업데이트로 키를 바꿀 수 있습니다.
- 바뀐 `src\UpdateKeys.cs`(공개키만 들어 있음)를 커밋합니다.
- 다른 저장소에서 같은 키를 쓰려면 그 저장소에서 `-UseExisting -SetSecret`으로 실행합니다.

### 버전마다

1. `VERSION`을 올리고 커밋합니다(예: `1.0.1`).
2. 같은 이름의 태그를 푸시합니다: `git tag v1.0.1; git push origin v1.0.1`
3. GitHub Actions가 빌드 → 서명 → 빌드된 exe로 서명 검증 → **초안(draft) 릴리스** 생성까지 합니다.
4. 초안을 확인하고 **Publish**를 누르면 그때부터 사용자에게 업데이트 알림이 갑니다.

## 확인한 환경

- Windows 11 (24H2 이후)
- 카카오톡 PC 26.9.2

카카오톡이 업데이트로 창 구조를 바꾸면 동작하지 않을 수 있습니다. 광고가 다시 보이면 카카오톡 버전과 함께 이슈로 알려 주세요.

## 면책

이 프로그램은 있는 그대로 제공되며, 사용에 따른 책임은 사용자에게 있습니다. 자세한 내용은 [LICENSE](LICENSE)를 보세요.
