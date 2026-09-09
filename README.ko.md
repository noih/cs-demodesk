# CS DemoDesk

[English](README.md) | [繁體中文](README.zh-TW.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | 한국어 | [Русский](README.ru.md)

Counter-Strike 2 데모용 Windows 데스크톱 도구 (포터블 단일 `.exe`). `.dem` 파일 하나로 매치 스탯, 하이라이트 영상 내보내기, 경기 전체의 2D 리플레이를 제공합니다.

## 기능

### 매치 스탯

교전, 유틸리티 사용, 다양한 라운드 상황을 바탕으로 경기 결과와 플레이어의 활약을 살펴봅니다. 비교 차트, 라운드별 추이, 반동 궤적으로 경기를 여러 관점에서 돌아볼 수 있습니다.

### 하이라이트 영상

- 하이라이트 (멀티킬, 클러치, 닌자 디퓨즈) 를 자동으로 찾아 점수를 매김
- 선택한 클립을 백그라운드의 CS2 로 녹화하고 FFmpeg 로 인코딩
- 해상도, FPS, H.264 / H.265, CPU 또는 NVIDIA
- HUD 요소별 표시 전환, 하나의 영상으로 합치기
- 채팅 앱 공유용 파일 크기 제한 (10 / 20 / 50 MB)

### 2D 리플레이

- 플레이어 위치, 시야 방향, 체력, 방탄복, 무기, 소지금
- 투척 무기, 스모크 / 화염 / 섬광 범위, C4 와 해체 카운트다운, 킬 피드, 가청 범위
- 라운드 이동, 재생 속도, 플레이어 따라가기
- 여러 층 맵은 층마다 패널로 표시. 레이더 이미지는 로컬 게임 파일에서 추출

## 게임 파일은 건드리지 않습니다

게임 폴더에는 아무것도 쓰지 않습니다. 플러그인, 스크립트, cfg 추가도, 게임 파일 변경도 없습니다. 녹화는 [HLAE](https://github.com/advancedfx/advancedfx) 를 통해 `-insecure` 로 별도의 CS2 프로세스를 띄우고 (HLAE 를 직접 쓰는 것과 동일), 게임 내장 netcon 콘솔로 명령을 보내며, 게임 설정은 별도의 `USRLOCALCSGO` 폴더에 두므로 플레이어 본인의 설정은 그대로입니다. 서드파티 도구는 모두 앱 자체 데이터 폴더에 다운로드됩니다.

| 도구 | 용도 | 출처 |
| --- | --- | --- |
| HLAE | 녹화 (mirv_streams) | [advancedfx/advancedfx](https://github.com/advancedfx/advancedfx) |
| FFmpeg | 인코딩, 합치기, 크기 제한 | [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) (GPL) |
| Source 2 Viewer CLI | vpk 에서 레이더 이미지 추출 | [ValveResourceFormat](https://github.com/ValveResourceFormat/ValveResourceFormat) (MIT) |
| demoparser | 데모 파싱 (vendored) | [LaihoE/demoparser](https://github.com/LaihoE/demoparser) (MIT) |

## 참고

- Windows 전용. CS2 가 설치되어 있어야 합니다.
- 녹화 중 CS2는 기본적으로 숨겨집니다. 내보내기 설정에서 "게임 화면 표시"를 켜면 볼 수 있습니다. Steam 계정 하나로는 CS2 를 동시에 하나만 실행할 수 있으므로 녹화 중에는 게임을 할 수 없습니다. 내보내기 작업은 한 번에 하나씩 실행되고 나머지는 대기합니다.
- 녹화용 CS2 는 `-insecure` 로 실행되어 VAC 서버에 접속할 수 없습니다. 녹화가 끝나면 종료되며 일반 실행에는 영향이 없습니다.
- CS2 업데이트로 HLAE 가 동작하지 않을 수 있습니다. HLAE 새 버전이 나오면 설정에서 도구를 다시 다운로드하세요.
- 채팅 앱이나 브라우저에서 재생하려면 H.264 를 선택하세요. NVIDIA 인코더는 NVIDIA GPU 가 필요합니다.

## 설계

데이터베이스가 없고 내부 상태를 최소한으로 유지합니다. 모든 것은 실행 파일 옆 `demodesk-data\` 아래의 일반 파일입니다. 설정과 내보내기 작업은 JSON 기록이고, 파싱 결과, 리플레이 스트림, 레이더 이미지는 버전이 붙은 캐시로 언제든 지울 수 있으며 데모, 스키마, 게임 버전이 바뀌면 다시 만들어집니다. 데모 목록은 새로고침할 때마다 리플레이 폴더를 다시 검색하므로 앱 밖에서 파일을 추가, 이동, 삭제해도 부작용이 없습니다.

## 빌드

[Rust](https://rustup.rs) (stable) 와 Visual Studio Build Tools (C++ 를 사용한 데스크톱 개발), Node.js 24.20.0 (`.nvmrc`), WebView2 (Windows 내장) 가 필요합니다.

```powershell
npm install
npm run app:dev      # 개발: Vite + Tauri 창
npm run app:build    # dist-portable\CS-DemoDesk-<version>.exe
npm run test:core    # Rust 단위 테스트
```

## 릴리스

릴리스는 [GitHub Actions](.github/workflows/release.yml) 로 빌드되며 build provenance 증명이 붙습니다. 다운로드한 파일이 이 저장소에서 빌드되었는지 확인하려면:

```powershell
gh attestation verify CS-DemoDesk-<version>.exe --owner noih
```

## 라이선스

Copyright (C) 2026 NOIH - <https://github.com/noih>

[GNU AGPL-3.0](LICENSE). 서드파티 구성 요소는 각자의 라이선스를 따릅니다 (위 표 참고. vendored 된 demoparser 는 `vendor/demoparser/LICENSE` 에 MIT 라이선스를 유지).
