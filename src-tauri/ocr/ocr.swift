// PasteNext OCR 辅助二进制(Swift / Vision 框架)
//
// 由 src-tauri/build.rs 在编译期用 `xcrun swiftc` 编译为 `ocr_helper`,
// 运行时被 Rust 侧通过 std::process::Command 调用:
//   ocr_helper <图片路径> [识别语言 ...]
// 标准输出返回 JSON: {"text": "识别出的多行文本"}
//
// 选用原生 Vision 框架:完全离线、无需任何外部依赖、中英文识别质量高。
import Foundation
import Vision
import ImageIO

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write(Data(msg.utf8))
    exit(1)
}

let args = CommandLine.arguments
guard args.count >= 2 else {
    fail("usage: ocr_helper <imagePath> [lang ...]\n")
}

let imagePath = args[1]
let langs = Array(args.dropFirst(2))

let url = URL(fileURLWithPath: imagePath)

// 用 ImageIO 把磁盘上的图片解码为 CGImage(Vision 直接吃 CGImage)
guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
      let cgImage = CGImageSourceCreateImageAtIndex(src, 0, nil) else {
    fail("cannot read image: \(imagePath)\n")
}

let request = VNRecognizeTextRequest()
if !langs.isEmpty {
    request.recognitionLanguages = langs
}
request.recognitionLevel = .accurate
request.usesLanguageCorrection = true

let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
do {
    try handler.perform([request])
} catch {
    fail("vision perform failed: \(error)\n")
}

guard let observations = request.results else {
    print("{\"text\":\"\"}")
    exit(0)
}

var lines: [String] = []
for obs in observations {
    guard let best = obs.topCandidates(1).first else { continue }
    let s = best.string.trimmingCharacters(in: .whitespacesAndNewlines)
    if !s.isEmpty { lines.append(s) }
}
let text = lines.joined(separator: "\n")

let payload: [String: Any] = ["text": text]
if let data = try? JSONSerialization.data(withJSONObject: payload),
   let json = String(data: data, encoding: .utf8) {
    print(json)
} else {
    print("{\"text\":\"\"}")
}
