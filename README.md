# KakaoAdHide

카카오톡 PC(Windows)의 광고 배너를 숨기는 작은 프로그램입니다.
`KakaoAdHide.exe`를 한 번 실행하면 설치가 끝나고, 이후 로그인할 때마다 카카오톡을 대신 켜서 광고를 숨깁니다.

> 비공식 프로젝트입니다. 카카오(Kakao Corp.)와 관련이 없습니다.

## 특징

- **광고 자리까지 없앰**: 광고 창을 숨기고, 친구/채팅 목록을 창 바닥까지 늘려서 처음부터 광고가 없는 것처럼 보입니다.
- **시작할 때도 안 보임**: 카카오톡이 켜지면서 잠깐 그리는 기본 배너까지 가립니다.
- **창 조절이 느려지지 않음**: 창을 끌거나 크기를 바꿀 때 카카오톡을 방해하지 않습니다.
- **카카오톡을 건드리지 않음**: 실행 파일 수정, 코드 주입, 통신 차단이 없습니다. 화면에 보이는 창만 일반 Windows API로 숨깁니다.
- **가벼움**: 창이 바뀔 때만 동작합니다(메모리 약 25MB). 관리자 권한이 필요 없습니다.

## 설치

1. [Releases](../../releases)에서 `KakaoAdHide.exe`를 받습니다.
2. 실행합니다. 설치가 끝나면 카카오톡이 광고 없이 켜집니다.

처음 실행할 때 **"Windows의 PC 보호"** 창이 뜨면 **추가 정보 → 실행**을 누르세요.
코드 서명을 하지 않은 프로그램이라 뜨는 경고입니다. 받은 파일이 Releases의 파일과 같은지는 함께 올린 `SHA256SUMS.txt`로 확인할 수 있습니다.

```powershell
Get-FileHash .\KakaoAdHide.exe -Algorithm SHA256
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

## 제거

**설정 > 앱 > 설치된 앱 > KakaoAdHide > 제거**

또는:

```powershell
& "$env:LOCALAPPDATA\KakaoAdHide\KakaoAdHide.exe" --uninstall
```

## 명령줄

| 명령 | 동작 |
|---|---|
| `KakaoAdHide.exe` | 처음: 설치 후 실행. 설치된 위치에서 실행하면 바로 실행 |
| `KakaoAdHide.exe --run` | 로그인할 때 쓰는 모드(카카오톡을 트레이로 시작) |
| `KakaoAdHide.exe --uninstall` | 제거 |
| `--quiet` | 설치/제거할 때 안내 창을 띄우지 않음 |

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

결과물은 `dist\KakaoAdHide.exe`와 `dist\SHA256SUMS.txt`입니다.
`v*` 태그를 푸시하면 GitHub Actions가 빌드해서 Releases에 올립니다.

## 확인한 환경

- Windows 11 (24H2 이후)
- 카카오톡 PC 26.9.2

카카오톡이 업데이트로 창 구조를 바꾸면 동작하지 않을 수 있습니다. 광고가 다시 보이면 카카오톡 버전과 함께 이슈로 알려 주세요.

## 면책

이 프로그램은 있는 그대로 제공되며, 사용에 따른 책임은 사용자에게 있습니다. 자세한 내용은 [LICENSE](LICENSE)를 보세요.
