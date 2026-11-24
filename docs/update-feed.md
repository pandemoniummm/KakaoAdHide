# 업데이트 정보 파일 형식 (Windows · macOS 공통)

두 플랫폼은 같은 형식, 같은 서명 방식, 같은 공개키로 업데이트를 확인합니다. 릴리스 워크플로의 서명 단계(`tools/feed-sign`) 하나가 두 파일을 모두 만듭니다.

| 플랫폼 | 업데이트 정보 (2.6.0부터) | 받는 파일 | 확인 코드 |
|---|---|---|---|
| Windows | `feed-windows.json` | `KakaoAdHide-windows.exe` | `src/feed.rs`, `src/updater.rs`, `src/platform/windows.rs` |
| macOS | `feed-macos.json` | `KakaoAdHide-macos.dmg` | `src/feed.rs`, `src/updater.rs`, `src/platform/macos.rs` |

앱은 `https://github.com/pandemoniummm/KakaoAdHide/releases/latest/download/<업데이트 정보>`를 읽습니다. 이 주소는 공개된 최신 릴리스만 가리키고, 초안은 가리키지 않습니다.

> 옛 버전 호환: 2.5.2는 `update-windows.json`·`update-macos.json`을, 2.5.1 이하는 `latest-windows.json`·`latest-macos.json`을 읽습니다. 이 파일들은 각각 징검다리 버전의 것으로 고정해 두고 릴리스마다 그대로 다시 올립니다. 그래서 옛 버전도 징검다리를 거쳐 최신 버전까지 올라옵니다([2.5.2](#징검다리-252), [2.6.0](#징검다리-260)).

## JSON

```json
{
  "version":   "1.1.0",
  "url":       "https://github.com/pandemoniummm/KakaoAdHide/releases/download/v1.1.0/KakaoAdHide-macos.dmg",
  "size":      1234567,
  "sha256":    "<받는 파일의 SHA-256, 16진수 소문자>",
  "signature_ed25519": "<base64>",
  "page":      "https://github.com/pandemoniummm/KakaoAdHide/releases/tag/v1.1.0",
  "units":     ["DAN-FTsOAPU4yZ6oHLGy"],
  "units_signature": "<base64>"
}
```

UTF-8로 읽습니다. 앞에 BOM이 있어도 받아들입니다.

## 서명

- 필드: `signature_ed25519` (2.5.1부터)
- 알고리즘: Ed25519
- 서명 대상: `"TalkNap\n" + version + "\n" + sha256` (UTF-8, sha256은 16진수 소문자). 2.6.0부터 앞머리가 `TalkNap`입니다. 옛 `KakaoAdHide` 앞머리 서명은 통과하지 못합니다.
- 버전과 파일 해시를 함께 서명하므로, 옛 릴리스 파일을 새 버전인 척 내밀어 되돌리는(다운그레이드) 것을 막습니다.
- 공개키 2개(주 키, 예비 키) 중 하나만 맞으면 통과합니다. 키는 `src/keys.rs`에 있습니다(32바이트, base64).
- 개인키(PKCS#8, base64): 주 키는 GitHub Secret `SIGNING_KEY_ED25519`(CI가 서명), 예비 키는 오프라인 보관. `tools/feed-sign`이 만들고 서명합니다([development.md](development.md#처음-한-번-업데이트-서명-키)).
- `feed-sign`은 서명한 뒤 `src/keys.rs`의 키로 다시 확인하고 나서야 파일을 씁니다. 앱과 같은 `src/feed.rs`를 그대로 가져다 씁니다.

### 징검다리 (2.5.2)

서명 방식이나 업데이트 정보 이름을 바꿀 때, 옛 버전이 끊기지 않게 한 버전을 거쳐 가게 합니다.

- 1.0.0 ~ 2.5.0은 `latest-*.json`의 `signature`(RSA-3072, PKCS#1 v1.5, SHA-256, 서명 대상은 위와 같음)만 확인하고, 모르는 필드는 무시합니다. 2.5.1은 같은 파일의 `signature_ed25519`를 확인합니다.
- **2.5.2 릴리스**만 `latest-*.json`을 RSA와 Ed25519로 함께 서명해 만들었습니다. 그 두 파일이 `packaging/bridge/`에 그대로 들어 있고, 그 뒤 릴리스는 CI가 이 파일을 **바꾸지 않고** 다시 올립니다. 2.5.2의 exe·dmg는 바뀌지 않으니 다시 서명할 필요가 없습니다. 이 파일은 고치지 않습니다(서명이 깨짐).
- 그래서 2.5.1 이하는 먼저 2.5.2로 올라오고, 2.5.2는 `update-*.json`을 따라 2.6.0(아래)으로 갑니다.
- 2.5.2 다음부터 RSA 서명 코드와 키는 없습니다. RSA는 이제 `packaging/bridge/`의 서명된 파일 두 개에만 남아 있습니다.
- 다음에 또 바꿀 일이 생기면 같은 방법을 씁니다: 새 방식을 아는 버전을 옛 방식으로도 서명해 징검다리로 내고, 옛 업데이트 정보를 그 버전에 고정합니다.

### 징검다리 (2.6.0)

2.6.0부터 앱은 `feed-*.json`을 `TalkNap` 서명 문구로 확인합니다. 2.5.2는 옛 문구(`KakaoAdHide\n…`)로 `update-*.json`을 확인하므로, 같은 방법으로 한 버전을 거쳐 갑니다.

- **2.6.0 릴리스**가 `update-*.json`을 옛 문구(`KakaoAdHide`)로 서명해 만들고, 이 파일이 2.6.0을 가리킵니다. 그 두 파일은 `packaging/bridge/`에 고정하고, 그 뒤 릴리스는 CI가 **바꾸지 않고** 다시 올립니다. 2.6.0의 exe·dmg는 바뀌지 않으니 다시 서명할 필요가 없습니다.
- 그래서 2.5.2는 `update-*.json`으로 2.6.0에 올라오고, 2.6.0부터는 `feed-*.json`을 따라 최신 버전으로 갑니다.
- 옛 문구는 앱 코드에 없습니다. `tools/feed-sign`에만 남아 징검다리 파일을 서명합니다.

## 광고 자리 ID 목록

카카오톡이 하단 배너의 광고 자리 ID(`DAN-…`)를 바꾸면 내장 배너가 다시 보입니다. 새 버전을 내지 않고 고칠 수 있게, 업데이트 정보에 ID 목록(`units`)을 따로 서명해서 싣습니다(2.5.0부터 읽음).

- 서명 대상: `"TalkNap-units\n" + 플랫폼 + "\n" + ID들을 "\n"으로 이은 것` (UTF-8). 플랫폼은 `windows` 또는 `macos`라, 한쪽 목록을 다른 쪽에 쓸 수 없습니다. 키와 알고리즘은 위 서명과 같습니다(Ed25519). 2.5.0은 이 서명을 RSA로 확인하므로 목록을 무시하고, 앱에 들어 있는 목록만 씁니다.
- 버전 서명과 따로 서명하므로, 목록만 바꿔도 버전 서명은 그대로입니다. 2.5.0 이전 버전은 이 필드를 무시합니다.
- 앱은 업데이트를 확인할 때마다 서명이 맞는 목록을 설정(`BannerUnits`)에 저장합니다. 이 목록은 앱에 들어 있는 목록(`ad-units/<플랫폼>.txt`)에 **더해서만** 씁니다. 서명이 틀리거나 ID 형식(`DAN-`와 영숫자, 20개 이하)이 아니면 무시합니다.
- Windows만 씁니다. macOS는 모든 자리에 빈 광고를 주므로 아직 쓰지 않습니다.
- `--verify-feed`는 목록이 있으면 그 서명도 확인합니다.

**ID가 바뀌었을 때**

1. `server.log`에서 새 `[DAN-…]`를 확인하고 `ad-units/<플랫폼>.txt`에 추가해 main에 푸시합니다(옛 ID는 남겨 둡니다).
2. Actions → `units` → Run workflow. 최신 릴리스의 `feed-*.json`에 새 목록을 서명해 다시 올립니다(`feed-sign units`). 2.5.2가 읽는 `update-*.json`에도 옛 문구로 같은 목록을 다시 서명합니다(`--legacy`). 가장 옛 징검다리 `latest-*.json`은 건드리지 않습니다.
3. 설치된 앱은 다음 업데이트 확인 때(시작 1분 뒤, 그 뒤 하루 한 번) 목록을 받습니다. 카카오톡을 다시 켜면 빈 광고가 돌아옵니다.

릴리스 때는 CI가 같은 도구로 목록을 함께 서명합니다.

## 앱이 확인하는 것

공통 (하나라도 틀리면 설치하지 않고 릴리스 페이지를 안내):

1. `url`이 `https://github.com/pandemoniummm/KakaoAdHide/releases/download/`로 시작(2.6.0부터는 새 저장소 이름 `.../TalkNap/releases/download/`도 허용)
2. 받은 파일의 크기와 SHA-256이 JSON과 같음
3. `signature_ed25519`가 공개키 중 하나로 검증됨
4. `version`이 지금 버전보다 높음

macOS는 여기에 더해 dmg 안의 앱을 확인합니다.

5. dmg를 읽기 전용으로 마운트하고, 볼륨 바로 아래의 `KakaoAdHide.app`(바로가기가 아닌 실제 폴더)을 복사
6. 번들 ID `io.github.pandemoniummm.KakaoAdHide`, 버전이 JSON의 `version`과 같음
7. 코드 서명이 번들 전체에서 유효하고, 릴리스 인증서(`KakaoAdHide Code Signing`, SHA-1은 `src/platform/macos.rs`)로 서명됨

그다음 앱이 종료되면 별도 스크립트가 새 앱을 옆에 복사하고 바꿔 끼웁니다. 실패하면 이전 앱을 되돌려 놓고, 어느 쪽이든 다시 실행합니다.

## 릴리스 워크플로의 확인

- Windows: 만든 exe가 `--verify-feed feed-windows.json KakaoAdHide-windows.exe`로 직접 확인 (exe에 들어간 버전과 JSON 버전이 같은지도 확인)
- macOS: dmg에서 꺼낸 앱이 `--verify-feed feed-macos.json KakaoAdHide-macos.dmg`로 직접 확인 (위 1~7을 사용자와 같은 코드로 실행)
- 징검다리 릴리스에서는 징검다리 파일도 확인합니다: 2.5.2는 `latest-*.json`, 2.6.0은 `update-*.json`(`feed-sign verify --legacy`). 그 뒤 릴리스의 이 파일들은 고정본을 복사한 것이라 확인하지 않습니다(버전이 다름).

## macOS 코드 서명 인증서

자체 서명 인증서라 무료지만, 처음 실행할 때 "그래도 열기"가 필요합니다. 업데이트는 이 인증서로 서명된 앱만 설치합니다.

- 만들기: `tools/new-macos-signing-cert.sh` (개인키는 `~/.keys/`, 저장소 밖)
- CI 등록: `tools/new-macos-signing-cert.sh --set-secret` → `MACOS_SIGNING_P12`, `MACOS_SIGNING_PASSWORD`
- **인증서를 바꾸면** 설치된 앱이 새 버전을 거부합니다(수동 업데이트 필요). 나중에 Apple Developer ID로 옮길 때는, 먼저 두 서명을 모두 받아들이는 버전(`src/platform/macos.rs`의 `requirement`에 팀 ID 추가)을 내보낸 다음에 서명을 바꿉니다.
