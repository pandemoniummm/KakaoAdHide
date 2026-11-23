# 징검다리 업데이트 정보 (2.5.2)

2.5.1 이하는 `latest-windows.json`·`latest-macos.json`을 읽습니다. 이 두 파일은 v2.5.2 릴리스에 올라간 것을 그대로 가져온 것이고, 그 뒤 모든 릴리스에 바꾸지 않고 다시 올립니다(`.github/workflows/build.yml`). 옛 버전은 이 파일로 2.5.2까지 올라온 뒤 `update-*.json`을 따라갑니다.

- 1.0.0 ~ 2.5.0은 RSA `signature`, 2.5.1은 `signature_ed25519`로 확인합니다.
- **고치지 마세요.** 한 글자라도 바뀌면 서명이 깨져서 옛 버전이 업데이트를 받지 못합니다.
- 자세한 내용: [docs/update-feed.md](../../docs/update-feed.md#징검다리-252)
