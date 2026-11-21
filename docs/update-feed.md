# 업데이트 정보 파일 형식 (Windows · macOS 공통)

두 플랫폼은 같은 형식, 같은 서명 방식, 같은 공개키로 업데이트를 확인합니다. 릴리스 워크플로의 서명 단계(`tools/sign-release.ps1`) 하나가 두 파일을 모두 만듭니다.

| 플랫폼 | 업데이트 정보 | 받는 파일 | 확인 코드 |
|---|---|---|---|
| Windows | `latest-windows.json` | `KakaoAdHide-windows.exe` | `src/feed.rs`, `src/updater.rs`, `src/platform/windows.rs` |
| macOS | `latest-macos.json` | `KakaoAdHide-macos.dmg` | `src/feed.rs`, `src/updater.rs`, `src/platform/macos.rs` |

앱은 `https://github.com/pandemoniummm/KakaoAdHide/releases/latest/download/<업데이트 정보>`를 읽습니다. 이 주소는 공개된 최신 릴리스만 가리키고, 초안은 가리키지 않습니다.

> 1.0.0 사용자 호환: `latest-windows.json`의 이름·형식, 서명 형식, 공개키는 바꾸면 안 됩니다.

## JSON

```json
{
  "version":   "1.1.0",
  "url":       "https://github.com/pandemoniummm/KakaoAdHide/releases/download/v1.1.0/KakaoAdHide-macos.dmg",
  "size":      1234567,
  "sha256":    "<받는 파일의 SHA-256, 16진수 소문자>",
  "signature": "<base64>",
  "page":      "https://github.com/pandemoniummm/KakaoAdHide/releases/tag/v1.1.0"
}
```

UTF-8로 읽습니다. 앞에 BOM이 있어도 받아들입니다.

## 서명

- 알고리즘: RSA-3072, PKCS#1 v1.5, SHA-256
- 서명 대상: `"KakaoAdHide\n" + version + "\n" + sha256` (UTF-8, sha256은 16진수 소문자)
- 버전과 파일 해시를 함께 서명하므로, 옛 릴리스 파일을 새 버전인 척 내밀어 되돌리는(다운그레이드) 것을 막습니다.
- 공개키 2개(주 키, 예비 키) 중 하나만 맞으면 통과합니다. 키는 `src/keys.rs`에 있고(KakaoAdHide 1.0.0과 같은 키), 테스트(`published_feed_verifies`)가 공개된 v1.0.0 업데이트 정보의 실제 서명을 이 키로 검증합니다.
- 개인키: 주 키는 GitHub Secret `SIGNING_PRIVATE_KEY`(CI가 서명), 예비 키는 오프라인 보관. `tools/new-signing-keys.ps1`가 두 키 파일을 함께 씁니다.

## 앱이 확인하는 것

공통 (하나라도 틀리면 설치하지 않고 릴리스 페이지를 안내):

1. `url`이 `https://github.com/pandemoniummm/KakaoAdHide/releases/download/`로 시작
2. 받은 파일의 크기와 SHA-256이 JSON과 같음
3. `signature`가 공개키 중 하나로 검증됨
4. `version`이 지금 버전보다 높음

macOS는 여기에 더해 dmg 안의 앱을 확인합니다.

5. dmg를 읽기 전용으로 마운트하고, 볼륨 바로 아래의 `KakaoAdHide.app`(바로가기가 아닌 실제 폴더)을 복사
6. 번들 ID `io.github.pandemoniummm.KakaoAdHide`, 버전이 JSON의 `version`과 같음
7. 코드 서명이 번들 전체에서 유효하고, 릴리스 인증서(`KakaoAdHide Code Signing`, SHA-1은 `src/platform/macos.rs`)로 서명됨

그다음 앱이 종료되면 별도 스크립트가 새 앱을 옆에 복사하고 바꿔 끼웁니다. 실패하면 이전 앱을 되돌려 놓고, 어느 쪽이든 다시 실행합니다.

## 릴리스 워크플로의 확인

- Windows: 만든 exe가 `--verify-feed latest-windows.json KakaoAdHide-windows.exe`로 직접 확인 (exe에 들어간 버전과 JSON 버전이 같은지도 확인)
- macOS: dmg에서 꺼낸 앱이 `--verify-feed latest-macos.json KakaoAdHide-macos.dmg`로 직접 확인 (위 1~7을 사용자와 같은 코드로 실행)

## macOS 코드 서명 인증서

자체 서명 인증서라 무료지만, 처음 실행할 때 "그래도 열기"가 필요합니다. 업데이트는 이 인증서로 서명된 앱만 설치합니다.

- 만들기: `tools/new-macos-signing-cert.sh` (개인키는 `~/.keys/`, 저장소 밖)
- CI 등록: `tools/new-macos-signing-cert.sh --set-secret` → `MACOS_SIGNING_P12`, `MACOS_SIGNING_PASSWORD`
- **인증서를 바꾸면** 설치된 앱이 새 버전을 거부합니다(수동 업데이트 필요). 나중에 Apple Developer ID로 옮길 때는, 먼저 두 서명을 모두 받아들이는 버전(`signing_requirement`에 팀 ID 추가)을 내보낸 다음에 서명을 바꿉니다.
