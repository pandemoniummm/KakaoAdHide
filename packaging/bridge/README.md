# 브릿지 버전 업데이트 정보

## 2.5.2

2.5.1 이하는 `latest-windows.json`·`latest-macos.json`을 읽습니다. 이 두 파일은 v2.5.2 릴리스에 올라간 것을 그대로 가져온 것이고, 그 뒤 모든 릴리스에 바꾸지 않고 다시 올립니다(`.github/workflows/build.yml`). 옛 버전은 이 파일로 2.5.2까지 올라온 뒤 `update-*.json`을 따라갑니다.

- 1.0.0 ~ 2.5.0은 RSA `signature`, 2.5.1은 `signature_ed25519`로 확인합니다.
- **고치지 마세요.** 한 글자라도 바뀌면 서명이 깨져서 옛 버전이 업데이트를 받지 못합니다.
- 자세한 내용: [docs/update-feed.md](../../docs/update-feed.md#브릿지-버전-252)

## 2.6.0

2.5.2는 `update-windows.json`·`update-macos.json`을 읽습니다(옛 문구 `KakaoAdHide`로 서명). 2.6.0부터는 `feed-*.json`을 읽습니다. `update-*.json`은 2.6.0 릴리스를 만들 때 CI가 옛 문구로 서명해 만든 것으로, 그 릴리스의 파일을 내려받아 여기에 넣었습니다. 그 뒤 릴리스에는 CI가 이 파일을 바꾸지 않고 다시 올립니다(광고 자리 목록 `units`만 새로 서명).

- `update-*.json`: v2.6.0 릴리스의 파일 그대로입니다. 고치지 않습니다.
- 자세한 내용: [docs/update-feed.md](../../docs/update-feed.md#브릿지-버전-260)

## 2.7.0

새 브릿지 파일은 없습니다. 2.7.0부터 릴리스 파일 이름이 `TalkNap.exe`·`TalkNap.dmg`로 바뀌지만, 위 두 브릿지 파일은 v2.5.2·v2.6.0 릴리스의 옛 이름 파일을 가리키므로 그대로 올립니다. 2.6.0은 `feed-*.json`을 옛 저장소 주소로 읽고 GitHub가 새 저장소(`TalkNap`)로 넘겨 줍니다. 그래서 `KakaoAdHide`라는 저장소를 다시 만들지 않습니다(자세한 내용: [docs/update-feed.md](../../docs/update-feed.md#270-이름을-talknap으로)).
