# 3D Graphics trên Windows

## Build từ source

Cần PowerShell, Node.js, Rustup, `wasm-bindgen` CLI và Visual Studio Build Tools với C++/Windows SDK để chạy test Rust native. Môi trường kiểm tra dùng Node.js 24.15.0, npm 12.0.2, Vite 6.3.5, Chrome 157.0.8103.51 và Rust 1.82.0-nightly (`8e86c9567`, toolchain `nightly-2024-08-02`). CLI 0.2.100 khớp với `wasm-bindgen` trong `Cargo.lock`.

Máy kiểm tra chưa có Rustup nên cài toolchain cục bộ. Từ thư mục gốc của repo trong PowerShell:

```powershell
New-Item -ItemType Directory -Force wasm-graphics/target/setup | Out-Null
$env:RUSTUP_HOME = "$PWD\wasm-graphics\target\rustup"
$env:CARGO_HOME = "$PWD\wasm-graphics\target\cargo"
curl.exe --fail --location https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe --output wasm-graphics/target/setup/rustup-init.exe
.\wasm-graphics\target\setup\rustup-init.exe -y --no-modify-path --profile minimal --default-toolchain nightly-2024-08-02 --component rust-src --target wasm32-unknown-unknown
$env:Path = "$env:CARGO_HOME\bin;$env:Path"
```

Nếu Rustup đã có sẵn, dùng `rustup toolchain install nightly-2024-08-02 --profile minimal --component rust-src --target wasm32-unknown-unknown` và giữ cấu hình Rustup của máy. Tiếp theo, dùng bản CLI Windows phát hành sẵn và build. [Bản phát hành 0.2.100](https://github.com/wasm-bindgen/wasm-bindgen/releases/tag/0.2.100)

```powershell
New-Item -ItemType Directory -Force wasm-graphics/target/setup | Out-Null
curl.exe --fail --location https://github.com/wasm-bindgen/wasm-bindgen/releases/download/0.2.100/wasm-bindgen-0.2.100-x86_64-pc-windows-msvc.tar.gz --output wasm-graphics/target/setup/wasm-bindgen.tar.gz
tar.exe -xf wasm-graphics/target/setup/wasm-bindgen.tar.gz -C wasm-graphics/target/setup
$env:Path = "$PWD\wasm-graphics\target\setup\wasm-bindgen-0.2.100-x86_64-pc-windows-msvc;$env:Path"
.\wasm-graphics\build.ps1
Push-Location web
npm.cmd ci --no-audit --no-fund
npm.cmd run build -- --outDir ../wasm-graphics/target/web-interactive
npm.cmd exec -- vite preview --host 127.0.0.1 --outDir ../wasm-graphics/target/web-interactive
```

`build.ps1` chạy Cargo release với lockfile, build lại standard library cho WASM có atomics và bulk memory, rồi sinh JS, TypeScript và WASM vào `web/wasm`. Bản đo dùng tối ưu mặc định của Cargo release. Cả Vite dev và preview cung cấp COOP/COEP; GitHub Pages giữ service worker cách ly hiện có. Trình duyệt cần `crossOriginIsolated === true` và `SharedArrayBuffer` cho cả cấu hình một worker. Trang hiển thị lỗi nếu điều kiện này hoặc khởi tạo WASM/Rayon thất bại.

## Walkthrough trong Chrome

1. Mở URL localhost Vite in ra, đóng hộp hướng dẫn và đợi cảnh Fantasy Book. Canvas đang hiển thị raster. Muốn thử một worker cho editor, mở một trang mới với `?threads=1`.
2. Mở **Add New Object**, chọn **Box**, đặt Length 2.5, Height 1.6 và nhấn **Add Object to Scene**. Editor tự chọn vật thể vừa thêm. Với **Custom (.glb)**, có thể dùng `web/public/static/roza_bust.glb`.
3. Mở **Edit Selected Object** → **Transform**. Thử tăng Position Z, Rotation X và Scale. Trong **Material Editor**, chọn Metal và đổi màu. Đổi **Field of View** để thử camera. Trong canvas, click để khóa chuột, dùng WASD và di chuột để đổi góc nhìn, rồi nhấn Escape.
4. Đặt **Samples per pixel** thành 2 và bấm **Start render**. Đợi **Complete · 2/2 spp**. **Back to editor** giữ cảnh để tiếp tục sửa. Thử đặt 4096 spp, bắt đầu và bấm **Cancel render**. Một lần chạy tiếp theo với cùng cảnh, camera và số mẫu sẽ cho cùng ảnh nhờ seed cố định.
5. Mở **Open benchmark**. Chọn A, B hoặc C. Mỗi lần đổi cấu hình, trang nạp lại để khởi tạo Rayon với số worker tương ứng. Chọn cảnh và thông số, nhấn **Start benchmark**. Tiến độ tính cả phần hàng đã xong trong mẫu hiện tại. Sau khi xong có thể tải JSON và PNG. JSON của lượt hủy hoặc hết giờ lưu trạng thái và số mẫu đã hoàn tất.

Kiểm tra tự động trong Chrome xác nhận các ảnh raster và path tracing thay đổi sau khi thêm box, di chuyển, xoay X, scale, đổi vật liệu, đổi FOV, di chuyển camera và import GLB. Sau khi hủy lượt 4.096 spp, render lại 2 spp cho PNG giống chính xác lượt 2 spp trước đó. [Biên bản](results/editor/walkthrough.json) · [Raster sau import](results/editor/raster.png) · [Path tracing 8 spp](results/editor/path-traced.png) · [Ảnh giao diện](results/editor/editor.png)

## Tái lập phép đo

Chạy trong `web` sau khi build source. Runner tự khởi động Vite preview với header cách ly và Chrome headless trong profile tạm. Dùng thư mục build sạch và thư mục kết quả mới cho mỗi lần chạy. Chạy hai cảnh tuần tự.

```powershell
$chrome = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
$build = '../wasm-graphics/target/web-interactive'
node scripts/benchmark.mjs $chrome $build small ../benchmarks/repro-small
node scripts/benchmark.mjs $chrome $build full ../benchmarks/repro-full
node scripts/benchmark-summary.mjs ../benchmarks/repro-full/runs.json
node scripts/smoke.mjs $chrome $build ../wasm-graphics/target/editor-check --walkthrough
node scripts/smoke.mjs $chrome $build ../wasm-graphics/target/preview-check --benchmark-preview
node scripts/smoke.mjs $chrome $build ../wasm-graphics/target/unsupported-check --unsupported
```

Tham số cuối tùy chọn của `benchmark.mjs` là spp và áp dụng chung cho A, B, C. Mỗi cấu hình có một lượt pilot với ngân sách 90 giây từ lúc bắt đầu tạo cảnh. Nếu pilot hết giờ, giữ JSON đó rồi thử lại cả ba cấu hình với spp thấp hơn trong thư mục mới. Với mỗi lượt, runner nạp trang mới và làm nóng bằng cảnh 640 tam giác ở 32×24, 1 spp. Sau ba pilot thành công và ảnh khớp, runner luân phiên thứ tự trong năm vòng `ABC / BCA / CAB / ACB / CBA`. Mỗi lượt được ghi vào `runs.json` ngay sau khi kết thúc.

### Cảnh và ảnh

- **Valley** là địa hình có dải sông thấp ở giữa, bờ và sườn đồi có màu khác nhau, cộng hai tam giác phát sáng trên cao. Lưới nhỏ có 19×16 ô, lưới lớn có 249×200 ô. Mỗi ô có hai tam giác, các ô ở cột đầu trừ ô cuối có bốn tam giác do chia qua tâm. Sau khi cộng đèn và chuyển đổi sang hình học path tracing, số tam giác lần lượt là 640 và 100.000. Test Rust kiểm tra diện tích dương và các bộ đỉnh duy nhất.
- Camera ở `(16, -18, 14)`, nhìn `(0, 0, 0.8)`, FOV 43°, không dùng defocus. Công thức chiều cao, màu, đèn, camera và hash nằm trong [benchmark.rs](../wasm-graphics/src/graphics/ray_tracing/benchmark.rs). `sceneHash` là FNV-1a 64-bit của hình học, màu, vật liệu và dữ liệu camera theo thứ tự cố định. `wasmSha256` nhận diện binary đã đo, `revision` là Git HEAD nền, `sourceSha256` nhận diện source làm việc tại thời điểm đo.
- Các lượt cùng cảnh dùng 96×72, độ sâu 5, seed 20261112 và cùng spp. Runner so từng kênh RGB tuyến tính trước gamma, với sai số tuyệt đối tối đa cho phép là `1e-6`. Nó kiểm tra số phần tử, giá trị hữu hạn và độ biến thiên, rồi lưu sai số lớn nhất, RMSE và SHA-256. Mỗi thư mục có một PNG hoàn tất cho từng cấu hình, còn từng lượt giữ hash PNG riêng.
- Ảnh xem trước 480×360, 32 spp dùng cấu hình C và cùng scene hash. Lượt này dành cho việc xem chi tiết cảnh. [Ảnh Valley](results/preview/valley-preview.png) · [Thông số và kiểm tra hủy](results/preview/preview.json)

### Đọc thời gian

| Trường | Phạm vi đo |
| --- | --- |
| `wasmLoadMs` | Tải, biên dịch và khởi tạo module WASM sau khi JS đã tải |
| `poolInitMs` | Khởi tạo pool và đợi worker Rayon sẵn sàng |
| `fixtureMs` | Sinh cảnh procedural, chuyển đổi và sao chép primitives dùng chung, thu thập đèn và tính hash |
| `accelBuildMs` | Gắn danh sách cho A, hoặc xây BVH phẳng cho B và C |
| `renderMs` | Tổng thời gian các lời gọi WASM render, gồm Rayon điều phối và đồng bộ |
| `renderWallMs` | Từ sau bước chuẩn bị tới lúc đạt spp, gồm thời gian nhường luồng chính và cập nhật tiến độ |
| `totalMs` | Tải WASM + khởi tạo pool + tạo cảnh + chuẩn bị + render theo đồng hồ; bỏ lượt làm nóng, điều hướng ban đầu, mã hóa PNG và xuất dữ liệu |

Chỉ diễn giải speedup khi phép so ảnh đạt, thông số giống nhau và mỗi cấu hình có đủ năm lượt. Lấy tỷ số của hai trung vị trong cùng cột thời gian. `totalMs` phù hợp khi mô tả chi phí của một lượt từ lúc bắt đầu tải renderer đến ảnh đủ mẫu. `renderMs` phù hợp khi mô tả chi phí tính ảnh đã có hình học và pool. Dữ liệu thô lưu cả pilot để kiểm tra sai số và trạng thái.

## Kết quả đo

Windows x64 build 26200, AMD Ryzen 5 5625U, Chrome headless 157.0.8103.51. Browser báo 12 logical processors. Số worker Rayon thực tế là 1, 1 và 8. Cả hai cảnh dùng 96×72, 2 spp, độ sâu 5 và seed 20261112. Mỗi dòng có năm lượt đo, kèm trung vị và khoảng nhỏ nhất–lớn nhất.

| Cảnh | Cấu hình | Tính ảnh (ms) | Đầu-cuối (ms) |
| --- | --- | ---: | ---: |
| 640 tam giác | A · brute-force, 1 | 238,67 [193,87–277,53] | 375,99 [273,50–443,40] |
| 640 tam giác | B · BVH, 1 | 31,35 [26,33–33,07] | 184,02 [99,23–226,60] |
| 640 tam giác | C · BVH, 8 | 9,64 [8,69–16,96] | 196,67 [96,74–217,09] |
| 100.000 tam giác | A · brute-force, 1 | 30.181,56 [26.945,90–31.189,06] | 30.310,60 [27.086,34–31.349,12] |
| 100.000 tam giác | B · BVH, 1 | 53,23 [50,91–55,07] | 336,65 [334,29–401,91] |
| 100.000 tam giác | C · BVH, 8 | 15,25 [14,08–15,71] | 329,34 [315,98–333,27] |

Trên cảnh 100.000 tam giác, A→C cải thiện trung vị đầu-cuối **92,03×** (30,311 s → 0,329 s). Riêng B→C cải thiện thời gian tính ảnh **3,49×**. Xây cây mất trung vị 168,23 ms ở B và 177,00 ms ở C, nên tổng thời gian hai cấu hình gần nhau ở 2 spp. Mốc 96→7 giây trong guideline là giả thuyết ban đầu. Các tỷ lệ trong bảng mô tả fixture và thông số trên.

Tất cả 36 lượt của bản bàn giao gồm pilot và lượt đo đã đủ 2 spp, sai số tuyệt đối lớn nhất và RMSE đều bằng 0. Hai scene hash lần lượt là `e380007e44d1ff26` và `54d0e73cca5b084c`.

- 640 tam giác. [Dữ liệu thô](results/small-final/runs.json) · [Tổng hợp](results/small-final/summary.json) · [PNG của C](results/small-final/C.png)
- 100.000 tam giác. [Dữ liệu thô](results/full-final/runs.json) · [Tổng hợp](results/full-final/summary.json) · [PNG của C](results/full-final/C.png)

Các lượt trước khi sửa cách nhường luồng được giữ để đối chiếu. Trên Chrome, cách dùng `scheduler.yield()` có thể trì hoãn việc cập nhật tiến độ và xử lý hủy. Bảng trên dùng kết quả đo lại sau khi chuyển sang timer và xác nhận hủy giữa lượt. [640 tam giác trước bản sửa](results/small/runs.json) · [100.000 tam giác trước bản sửa](results/full/runs.json)

## Phần đóng góp

- Rasterizer, vật liệu, lấy mẫu ánh sáng, BVH phẳng theo trục dài nhất và Rayon theo hàng đã có trong repo. A thêm duyệt tuần tự cùng danh sách primitives với B/C. Cả tia chính, tia nảy và tia bóng dùng chung logic hit và giới hạn khoảng cách. Nếu hai primitive có cùng `t` chính xác, primitive đứng sau trong thứ tự cảnh thắng; test so khoảng cách với dung sai `1e-5` và mã vật liệu.
- Một tia nhìn vào bờ sông chỉ có thể chạm các tam giác trong các hộp mà nó đi xuyên qua. A thử từng tam giác. BVH của B và C kiểm tra hộp trước, bỏ nguyên nhóm ở sườn đồi nằm ngoài tia, rồi dùng hit gần nhất để giới hạn khoảng cần tìm. Bước xây cây tốn thêm thời gian và bộ nhớ; lượt render ngắn có thể kết thúc trước khi khoản tiết kiệm đó bù được chi phí khởi tạo.
- Rayon phân phối các hàng độc lập cho worker. C dùng tám worker cho chính công việc của B. Mỗi pixel và chỉ số mẫu định danh một stream ngẫu nhiên dùng cho camera, vật liệu và đèn, nên thay đổi thứ tự chạy hàng vẫn cho cùng kết quả. Thêm worker có thể tăng chi phí khởi tạo, đồng bộ và tranh chấp bộ nhớ.
- Editor và benchmark dùng cùng hàm render theo nhóm tám hàng. Editor gom nhóm trong một khung hình khoảng 12 ms nếu có thể. Benchmark dùng một timer ngắn sau từng nhóm để Chrome có thể cập nhật React và xử lý nút hủy. Thời gian chờ timer nằm trong `renderWallMs` và `totalMs`. Một nhóm đang chạy hoặc bước xây cây có thể mất lâu hơn; hủy được xử lý ở điểm nhường kế tiếp. Editor khóa chỉnh sửa khi đang hiển thị lượt render và xóa mẫu khi bắt đầu lượt mới. Thay đổi hình học hay vật liệu làm mất hiệu lực BVH; đổi camera chỉ cần render lại từ mẫu đầu.

## Kiểm tra Rust

Trong Developer PowerShell của Visual Studio, từ repo:

```powershell
cargo +nightly-2024-08-02 test --manifest-path wasm-graphics/Cargo.toml --locked --lib
git diff --check
```

Các test native chạy đường hình học và shading thuần Rust. Vite build kiểm tra bundling. Browser walkthrough kiểm tra các binding WASM thực, điều khiển React, ảnh canvas và thông báo lỗi khi mô phỏng thiếu `SharedArrayBuffer`. `npm test` của repo vẫn là placeholder. Phép đo và walkthrough đã kiểm tra trên Chrome/Windows. Các trình duyệt khác và bản triển khai GitHub Pages chưa được kiểm tra trong lượt này.
