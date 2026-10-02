# Rust Pose Studio

A lightweight desktop **drawing mannequin** for figure-drawing practice, written in Rust with
[egui/eframe](https://github.com/emilk/egui). Pick one of 25 built-in floor poses, orbit the
camera to look at it from any angle, adjust the pose by dragging joints (with two-bone IK for
hands and feet), and export clean line-art reference images or a numbered contact sheet.

[繁體中文說明](#繁體中文說明) · [Download (Windows installer / portable exe)](https://github.com/stevenke1981/rust-pose-studio/releases/latest)

![Rust Pose Studio main window](docs/screenshot.png)

## Features

- **3D posable mannequin** – 17-joint skeleton (pelvis root → waist → chest → neck → head,
  shoulders/upper arms/forearms/hands, thighs/shins/feet). Joint rotations are Euler angles
  evaluated with forward kinematics; the body is built from capsules and ellipsoids and drawn
  as mannequin-style line art (white paper, black outlines, depth-sorted filled segments,
  depth shading, oval head with face-direction guide lines, soft contact shadows).
- **Camera** – orbit, pan and zoom; perspective or orthographic projection; front / ¾ /
  side / back / high-angle / top presets; one-click framing.
- **25 built-in presets** shown as a live-rendered thumbnail gallery – seiza, kneeling looking
  back, side-lying propped on an elbow, lying on the stomach with legs up, all fours, sitting
  hugging knees, side-saddle, crouching on toes, cat stretch, lying on the back with a knee
  raised, cross-legged, curled up, one-knee kneel, crawling, supine twist and more. Every
  preset is grounded on the floor (knees, shins, feet, hands or body actually touch it).
- **Manual posing**
  - click a joint handle or body part to select it;
  - drag blue handles to rotate a joint (FK), drag the green diamonds at wrists and ankles to
    place hands and feet with **two-bone IK** (elbow/knee bend direction is preserved),
    drag the purple pelvis handle to move the whole figure;
  - sliders for the selected joint's X/Y/Z rotation (elbows/knees are hinge joints);
  - reset joint, copy a joint to the other side, mirror the whole pose, random pose,
    standing pose, **undo/redo**, optional automatic snapping to the floor.
- **Body proportions** – height, head size, shoulder width, hip width and body type
  (slim / average / curvy). Proportions never change the joint angles.
- **Save / load poses** as small human-readable JSON files.
- **Export** (rendered by a CPU rasteriser built on `tiny-skia`, so it does not depend on the GPU
  and matches the on-screen style): PNG of the current view at 512–4096 px, white or
  transparent background, shadow / grid / no floor; and a **contact sheet** of all presets
  (or a Ctrl+click selection) with numbers and names. Exports run on a background thread.
- **Headless CLI** for scripting/batch export (see below).

![Contact sheet of all 25 presets](docs/contact-sheet.png)

Turnaround export (`--turnaround`) of preset 13:

![Turnaround](docs/turnaround.png)

## Controls

| Action | How |
| --- | --- |
| Select a joint | click its handle or the body part |
| Rotate a joint (FK) | drag a blue handle |
| Place hand / foot (IK) | drag the green diamond at the wrist / ankle |
| Move the figure | drag the purple pelvis handle |
| Orbit camera | drag empty space with the left button |
| Pan | right- or middle-drag |
| Zoom | mouse wheel |
| Undo / redo | Ctrl+Z / Ctrl+Y (or Ctrl+Shift+Z) |
| Save / open pose JSON | Ctrl+S / Ctrl+O |
| Export PNG | Ctrl+E |
| Mirror pose / reset joint / frame figure | M / R / F |
| Load preset / pick for contact sheet | click / Ctrl+click a thumbnail |

## Command line

```text
rust-pose-studio                         start the GUI
rust-pose-studio --contact-sheet OUT.png [--cell PX] [--cols N] [--yaw DEG] [--pitch DEG]
                 [--transparent] [--no-names] [--title TEXT] [--only 1,5,9]
rust-pose-studio --render POSE OUT.png   POSE = preset number (1-25) or a pose .json file
                 [--size PX] [--yaw DEG] [--pitch DEG] [--transparent] [--grid] [--turnaround]
rust-pose-studio --list                  list the built-in presets
rust-pose-studio --floor-report          show which body parts touch the floor per preset
rust-pose-studio --help | --version
```

## Pose file format

```json
{
  "format": "rust-pose-studio/pose",
  "version": 1,
  "name": "Seiza (kneeling on heels)",
  "root": [0.0, 0.52, 0.0],
  "joints": { "pelvis": [0, 0, 0], "thigh_l": [-90, 0, 0], "shin_l": [150, 0, 0] },
  "proportions": { "height": 1.7, "head_size": 1.0, "shoulder_width": 1.0, "hip_width": 1.0, "body_type": "average" },
  "camera": { "yaw": 30, "pitch": 12 }
}
```

Angles are degrees (X, Y, Z Euler angles relative to the parent joint, applied as Rz·Ry·Rx),
`root` is the pelvis position in metres (Y up, the figure faces +Z, +X is the figure's left).
Missing joints default to 0°, unknown keys are ignored; `proportions` and `camera` are optional.

## Install

- **Windows**: download `rust-pose-studio-<version>-x64-setup.exe` from
  [Releases](https://github.com/stevenke1981/rust-pose-studio/releases). It installs to
  `C:\Program Files\Rust Pose Studio`, adds Start Menu shortcuts, and (optional, on by
  default) adds the install folder to the system `PATH`. A portable `.exe` is attached too.
- **Linux / macOS**: build from source.

## Build from source

Requires Rust 1.95+ (edition 2024).

```bash
# Debian/Ubuntu build dependencies
sudo apt-get install pkg-config libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev libgl1-mesa-dev libwayland-dev

cargo build --release          # target/release/rust-pose-studio
cargo test                     # FK, IK, JSON round-trip, preset validity, PNG export
cargo clippy --all-targets
```

Windows installer (NSIS 3): `cargo build --release --target x86_64-pc-windows-msvc`, then
`makensis /DVERSION=0.1.0 installer\windows\rust-pose-studio.nsi` (writes to `dist\`).
Tagging `v*` builds the installer and the portable exe in GitHub Actions and attaches them to
the release.

## Project layout

| File | Purpose |
| --- | --- |
| `src/math.rs` | small Vec3 / Mat3 / Euler helpers |
| `src/skeleton.rs` | joints, proportions, poses, forward kinematics, mirroring |
| `src/body.rs` | capsule/ellipsoid body volumes, floor snapping, drag-handle positions |
| `src/ik.rs`, `src/posing.rs` | two-bone IK solver, limb aiming, random poses |
| `src/presets.rs` | the 25 built-in poses (authored with a small pose-builder DSL) |
| `src/render.rs` | camera, projection, depth-sorted 2D draw list shared by GUI and export |
| `src/raster.rs`, `src/export.rs` | tiny-skia rasteriser, PNG and contact-sheet export |
| `src/pose_io.rs` | pose JSON |
| `src/cli.rs` | headless command line |
| `src/app/` | the egui application |

## Credits

The built-in poses are original pose data **inspired by common figure-drawing reference pose
types** (kneeling, sitting, lying, crouching, all fours…). No third-party artwork is included or
traced; all images are rendered by this program. UI font: Ubuntu Light (bundled with egui).

## License

[MIT](LICENSE) © 2026 Ke Sheng Da

---

## 繁體中文說明

**Rust Pose Studio** 是一個用 Rust + egui 撰寫的輕量桌面「繪畫用木頭人／人體模型」工具，
適合人物速寫與姿勢練習：選擇 25 個內建的地面姿勢、從任意角度旋轉觀看、拖曳關節調整姿勢，
並匯出乾淨的線稿參考圖或帶編號的姿勢總表（contact sheet）。

### 功能

- **3D 可動人偶**：17 個關節的骨架（骨盆為根節點 → 腰 → 胸 → 頸 → 頭，肩／上臂／前臂／手、
  大腿／小腿／腳），以尤拉角 + 正向運動學（FK）計算；身體由膠囊體與橢球組成，
  以人偶線稿風格繪製（白底、黑色輪廓、依深度排序的填色部件、深度明暗、
  帶有臉部方向十字線的橢圓頭部、地面接觸陰影）。
- **攝影機**：環繞、平移、縮放；透視／正交投影；正面、3/4、側面、背面、俯視等預設視角。
- **25 個內建姿勢**（縮圖即時渲染）：正坐、跪坐回頭、側躺手肘撐地、趴姿抬腿、四肢著地、
  抱膝坐、側坐、蹲踞、貓式伸展、仰躺單膝立起、盤腿、蜷縮側躺、單膝跪地、爬行、仰躺扭轉等。
  每個姿勢的膝蓋、小腿、腳、手或身體都確實接觸地面。
- **手動調整**：點選關節；拖曳藍色控制點旋轉關節（FK）；拖曳手腕／腳踝的綠色菱形以
  **雙骨 IK** 擺放手腳；拖曳紫色骨盆點移動整個人偶；選取關節的 X/Y/Z 旋轉滑桿
  （手肘、膝蓋為鉸鏈關節）；重設關節、複製到另一側、整體左右鏡像、隨機姿勢、
  **復原／重做**、自動貼地。
- **身體比例**：身高、頭部大小、肩寬、臀寬、體型（纖細／標準／豐滿）。
- **姿勢存檔／讀取**：可讀的 JSON 格式。
- **匯出**：使用 CPU 光柵化（tiny-skia），不依賴 GPU，且與畫面風格一致；可匯出目前視角
  的 PNG（512–4096 px、白底或透明背景），以及所有（或以 Ctrl+點選挑選的）姿勢的編號總表。
  匯出在背景執行緒進行，不會卡住介面。
- **命令列模式**：可批次輸出圖片（見上方 Command line）。

### 操作

| 動作 | 方式 |
| --- | --- |
| 選取關節 | 點擊控制點或身體部位 |
| 旋轉關節（FK） | 拖曳藍色控制點 |
| 擺放手／腳（IK） | 拖曳手腕／腳踝的綠色菱形 |
| 移動人偶 | 拖曳紫色骨盆控制點 |
| 環繞視角 | 在空白處以左鍵拖曳 |
| 平移／縮放 | 右鍵或中鍵拖曳／滑鼠滾輪 |
| 復原／重做 | Ctrl+Z ／ Ctrl+Y |
| 儲存／開啟姿勢 | Ctrl+S ／ Ctrl+O |
| 匯出 PNG | Ctrl+E |
| 鏡像／重設關節／對焦人偶 | M ／ R ／ F |

### 安裝

- **Windows**：到 [Releases](https://github.com/stevenke1981/rust-pose-studio/releases) 下載
  `rust-pose-studio-<版本>-x64-setup.exe`，會安裝到 `C:\Program Files\Rust Pose Studio`、
  建立開始功能表捷徑，並（預設勾選）把安裝目錄加入系統 `PATH`；另附免安裝版 exe。
- **Linux / macOS**：請依上方說明自行編譯（`cargo build --release`）。

### 版權

內建姿勢為本專案自行建立的姿勢資料，**靈感來自常見的人物速寫參考姿勢類型**；
專案中不包含、也未描摹任何第三方畫作，所有圖片皆由本程式渲染。
授權：[MIT](LICENSE) © 2026 Ke Sheng Da
