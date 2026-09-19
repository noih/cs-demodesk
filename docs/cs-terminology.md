# CS terminology

The six UI locales use CS player vocabulary. Keep common abbreviations such as
ADR, KAST, HS%, CT, T, AWP, HE, HUD and 2k/3k/4k. Translate surrounding labels;
do not expose internal event names as feature names.
The spray-control legend uses Player / Spray pattern (玩家 / 壓槍軌跡), following
the user's preference for the familiar CS term. The chart explanation must state
that the reference pattern is inverted to guide spray control; it is not the
raw bullet-impact pattern.

| Meaning | English | 繁體中文 | 简体中文 | 日本語 | 한국어 | Русский |
| --- | --- | --- | --- | --- | --- | --- |
| Compensating for recoil | Spray control | 壓槍 | 压枪 | リコイルコントロール | 반동 제어 | Контроль спрея |
| Sustained fire | Spray | 壓槍 | 压枪 | スプレー | 스프레이 | Спрей |
| Killing a teammate's killer | Trade kill | 補槍 | 补枪 | トレードキル | 트레이드 킬 | Размен |
| Last player against enemies | Clutch | 殘局 | 残局 | クラッチ | 클러치 | Клатч |
| Shooting through a surface | Wallbang | 穿牆 | 穿墙 | 壁抜き | 월샷 | Прострел |
| Killing through smoke | Smoke kill | 穿煙擊殺 | 穿烟击杀 | スモーク抜きキル | 연막샷 킬 | Убийство через смок |
| Kill while flashed | Blind kill | 致盲擊殺 | 致盲击杀 | フラッシュ中のキル | 섬광에 맞은 상태에서 킬 | Убийство в ослеплении |
| Hidden defuse | Ninja defuse | 偷拆 | 偷拆 | ニンジャ解除 | 닌자 해체 | Ниндзя-дефьюз |
| Repeated jumps | Bhop | 連跳 | 连跳 | バニーホップ | 버니합 | Баннихоп |

These are UI wording choices, not claims that each language has only one valid
term. Prefer existing local usage over literal translations or forced English.
The Chinese blind-kill wording and 壓槍 / 压枪 for spray follow the user's explicit preference.

## Preserve the meaning of the data

- Recoil is the weapon's kick; spray control is the player's compensation.
  The chart shows sampled aim movement, not bullet impacts.
- A spray sample here requires at least three rifle shots. This is the app's
  sample definition, not a universal CS definition.
- Opening kills are the first enemy kills of a round, not necessarily entry kills.
- Surviving a lost round does not prove a deliberate save.
- A blind flag does not prove a fully white screen. Keep this limitation in the
  explanation even when the label says blind kill.
- Keep "possible" on ninja defuses: an enemy being alive does not prove stealth.
- Unusual aim or movement is an observation, not a cheating verdict. Keep
  technical descriptions where no ordinary gameplay term captures the criterion.
- Backend rule IDs, persisted titles, weapon IDs and calculation thresholds are
  independent of translated UI labels. Do not rename them for wording changes.

## Usage references

Reviewed on 2026-09-19. These sources provide usage examples, not a formal
multilingual standard or verification of this app's statistical definitions.

- [HLTV: How to watch Counter-Strike](https://www.hltv.org/news/38480/how-to-watch-counter-strike): trades, clutches, utility and stats.
- [kneel: The Ultimate CS2 Spray Control Guide](https://www.youtube.com/watch?v=vl6J1cQcErU): spray control, recoil reset and spray transfers.
- [CS2 日本語 Wiki: 用語集](https://cs2wiki.jp/glossary) and [FAQ](https://cs2wiki.jp/faq): Japanese player vocabulary and リコイルコントロール.
- [OP.GG Korean spray practice](https://op.gg/ko/cs2/spray-patterns/bizon): 연사 and 반동 제어.
- [Steam Community: Основы стрельбы в CS2](https://steamcommunity.com/sharedfiles/filedetails/?id=3053025615): Russian spray terminology.
- [PTT discussion of CS2 recoil](https://www.pttweb.cc/bbs/Steam/M.1693577696.A.DA2): Traditional Chinese 壓槍 and spray usage.
- [CS2 shooting practice guide](https://www.bilibili.com/video/BV1BC4y1Y7Pp/): Simplified Chinese 压枪 and 扫射转移.

Run `node scripts/test-locales.mjs` to check keys, plural forms, interpolation
parameters and tag fallback, then `npm run build` and `npm run test:ui` for UI changes.
