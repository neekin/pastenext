import Foundation
import AVFoundation
import ImageIO
import UniformTypeIdentifiers

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data(message.utf8)); exit(1)
}
let args = CommandLine.arguments
guard args.count == 6 else { fail("Usage: media_export input output format quality fps") }
let input = URL(fileURLWithPath: args[1]), output = URL(fileURLWithPath: args[2])
let format = args[3], quality = Int(args[4]) ?? 90, fps = min(20, max(5, Int(args[5]) ?? 10))
let asset = AVURLAsset(url: input)
if format == "gif" {
    let duration = CMTimeGetSeconds(asset.duration)
    guard duration.isFinite && duration > 0 else { fail("视频没有可导出的画面") }
    guard duration <= 120 else { fail("GIF 最长支持 120 秒，请改用 MP4 或 MOV 导出较长录屏") }
    let generator = AVAssetImageGenerator(asset: asset)
    generator.appliesPreferredTrackTransform = true
    generator.maximumSize = CGSize(width: quality >= 90 ? 960 : 640, height: quality >= 90 ? 960 : 640)
    generator.requestedTimeToleranceBefore = CMTime(value: 1, timescale: Int32(fps * 2))
    generator.requestedTimeToleranceAfter = generator.requestedTimeToleranceBefore
    let count = max(1, Int(ceil(duration * Double(fps))))
    guard let destination = CGImageDestinationCreateWithURL(output as CFURL, UTType.gif.identifier as CFString, count, nil) else { fail("无法创建 GIF 文件") }
    CGImageDestinationSetProperties(destination, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
    do {
        for index in 0..<count {
            try autoreleasepool {
                let time = CMTime(seconds: min(duration - 0.001, Double(index) / Double(fps)), preferredTimescale: 600)
                let image = try generator.copyCGImage(at: time, actualTime: nil)
                CGImageDestinationAddImage(destination, image, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: 1.0 / Double(fps)]] as CFDictionary)
            }
        }
    } catch { try? FileManager.default.removeItem(at: output); fail("GIF 转换失败：\(error.localizedDescription)") }
    guard CGImageDestinationFinalize(destination) else { try? FileManager.default.removeItem(at: output); fail("GIF 写入失败") }
} else {
    guard format == "mp4" || format == "mov" else { fail("不支持的视频格式") }
    let preset = quality >= 90 ? AVAssetExportPresetHighestQuality : AVAssetExportPreset1280x720
    guard let session = AVAssetExportSession(asset: asset, presetName: preset) else { fail("无法创建视频导出会话") }
    session.outputURL = output
    session.outputFileType = format == "mp4" ? .mp4 : .mov
    session.shouldOptimizeForNetworkUse = true
    let semaphore = DispatchSemaphore(value: 0)
    session.exportAsynchronously { semaphore.signal() }
    semaphore.wait()
    guard session.status == .completed else { try? FileManager.default.removeItem(at: output); fail(session.error?.localizedDescription ?? "视频导出失败") }
}
print("ok")
