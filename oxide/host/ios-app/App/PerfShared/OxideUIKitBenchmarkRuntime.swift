import AVFoundation
import Foundation
import ImageIO
import Metal
import os.signpost
import QuartzCore
import UIKit

private let perfSignpostSubsystem = "com.oxide.perf"
private let perfSignpostCategory = OSLog.Category.pointsOfInterest.rawValue

private let perfSignpostLog = OSLog(
    subsystem: perfSignpostSubsystem,
    category: perfSignpostCategory
)

let parkedCaseEnv = "OXIDE_PERF_CASE"
let perfUIKitLaunchEnv = "OXIDE_PERF_UIKIT_LAUNCH"
let perfLaunchScenarioEnv = "OXIDE_PERF_LAUNCH_SCENARIO"
let perfLaunchRouteEnv = "OXIDE_PERF_LAUNCH_ROUTE"
let perfTraceHandshakeEnv = "OXIDE_PERF_TRACE_HANDSHAKE"
let perfOxideRunnerEnv = "OXIDE_PERF_RUNNER"
let perfOxideRunnerSmokeEnv = "OXIDE_PERF_RUNNER_SMOKE"
let perfRefreshModeEnv = "OXIDE_PERF_REFRESH_MODE"
let perfMeasureIterationsEnv = "OXIDE_PERF_MEASURE_ITERATIONS"
let perfBenchmarkIterationsEnv = "OXIDE_PERF_BENCHMARK_ITERATIONS"
let perfTraceSettleMsEnv = "OXIDE_PERF_TRACE_SETTLE_MS"
let perfCameraTracePhasesEnv = "OXIDE_PERF_CAMERA_TRACE_PHASES"
let perfCameraMaxDrawableCountEnv = "OXIDE_PERF_CAMERA_MAX_DRAWABLE_COUNT"
let perfCameraPreviewSurfaceScaleEnv = "OXIDE_PERF_CAMERA_PREVIEW_SURFACE_SCALE"
let perfCameraCaptureContractModeEnv = "OXIDE_PERF_CAMERA_CAPTURE_CONTRACT_MODE"
let perfCameraStageMeasurementEnv = "OXIDE_PERF_CAMERA_STAGE_MEASUREMENT"
let perfCameraRealAppHostEnv = "OXIDE_PERF_CAMERA_REAL_APP_HOST"
let perfCameraRealAppHybridVisiblePreviewEnv = "OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW"
let perfUIKitLaunchArg = "-oxide-perf-uikit-launch"
let perfLaunchScenarioArg = "-oxide-perf-launch-scenario"
let perfLaunchRouteArg = "-oxide-perf-launch-route"
let readyNotificationName = "com.oxide.perf.ready"
let startNotificationName = "com.oxide.perf.start"
let completeNotificationName = "com.oxide.perf.complete"
let oxidePerfReportBeginLine = "OXIDE_PERF_REPORT_BEGIN"
let oxidePerfReportChunkPrefix = "OXIDE_PERF_REPORT_CHUNK "
let oxidePerfReportEndLine = "OXIDE_PERF_REPORT_END"
let oxideStageSummaryPrefix = "OXIDE_STAGE_SUMMARY "
let oxideCameraContractSummaryPrefix = "OXIDE_CAMERA_CONTRACT_SUMMARY "
let oxidePreviewPlanSummaryPrefix = "OXIDE_PREVIEW_PLAN_SUMMARY "
let oxideMemorySummaryPrefix = "OXIDE_MEMORY_SUMMARY "
let oxideTickDebugSummaryPrefix = "OXIDE_TICK_DEBUG_SUMMARY "
let oxideAppHostDebugSummaryPrefix = "OXIDE_APP_HOST_DEBUG_SUMMARY "

private let benchmarkCameraTargetWidth: Int32 = 1280
private let benchmarkCameraTargetHeight: Int32 = 720
private let benchmarkCameraTargetFps: Int32 = 30
private let oxideCameraPreviewReasonResize: Int32 = 1 << 2
private let oxideCameraPreviewReasonNoCurrentFrame: Int32 = 1 << 6
private let oxideCameraPreviewReasonNewTimestamp: Int32 = 1 << 7
private let oxideCameraPreviewReasonNewGeneration: Int32 = 1 << 8
private let oxideCameraPreviewReasonMeasuredPassMask: Int32 =
    oxideCameraPreviewReasonResize |
    oxideCameraPreviewReasonNoCurrentFrame |
    oxideCameraPreviewReasonNewTimestamp |
    oxideCameraPreviewReasonNewGeneration

enum OxideCameraCaptureContractMode: String
{
    case inputPriority = "input-priority"
    case preset720p = "preset-720p"

    var sessionPresetName: String
    {
        switch self
        {
        case .inputPriority:
            return "inputPriority"
        case .preset720p:
            return "hd1280x720"
        }
    }
}

private struct OxideCameraContractSummaryPayload: Codable
{
    let source: String
    let transport: String
    let devicePosition: String
    let sessionPreset: String
    let requestedPixelFormat: String
    let activePixelFormat: String
    let requestedWidth: Int32
    let requestedHeight: Int32
    let requestedFps: Int32
    let activeWidth: Int32
    let activeHeight: Int32
    let activeFps: Double
    let videoRange: String
    let colorSpace: String
    let wideColorAuto: Bool
    let mirrored: Bool
}

private func encodeCameraContractSummaryLine(
    _ payload: OxideCameraContractSummaryPayload
) -> String?
{
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    guard let data = try? encoder.encode(payload),
          let json = String(data: data, encoding: .utf8)
    else
    {
        return nil
    }
    return "\(oxideCameraContractSummaryPrefix)\(json)"
}

private func oxideCameraPixelFormatName(_ pixelFormat: FourCharCode) -> String
{
    switch pixelFormat
    {
    case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange:
        return "420f"
    case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
        return "420v"
    case kCVPixelFormatType_32BGRA:
        return "bgra8"
    default:
        return String(format: "%08X", pixelFormat)
    }
}

private func oxideCameraVideoRangeName(videoRange: UInt8) -> String
{
    videoRange == 1 ? "video" : "full"
}

private func oxideCameraVideoRangeName(pixelFormat: FourCharCode) -> String
{
    switch pixelFormat
    {
    case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
        return "video"
    default:
        return "full"
    }
}

private func oxideCameraColorSpaceName(colorSpace: UInt8) -> String
{
    switch colorSpace
    {
    case 1:
        return "display-p3"
    default:
        return "srgb"
    }
}

private func oxideCameraColorSpaceName(_ colorSpace: AVCaptureColorSpace) -> String
{
    switch colorSpace
    {
    case .P3_D65:
        return "display-p3"
    default:
        return "srgb"
    }
}

private func oxideCameraFrameAdvanceCount(
    previousGeneration: UInt64,
    previousTimestampNs: UInt64,
    currentGeneration: UInt64,
    currentTimestampNs: UInt64
) -> Int
{
    if currentGeneration > previousGeneration
    {
        let delta = currentGeneration - previousGeneration
        return delta > UInt64(Int.max) ? Int.max : Int(delta)
    }
    if currentGeneration == previousGeneration && currentTimestampNs > previousTimestampNs
    {
        return 1
    }
    if currentGeneration == 0 && previousGeneration == 0 && currentTimestampNs > previousTimestampNs
    {
        return 1
    }
    return 0
}

private func oxideCameraFps(from frameDuration: CMTime) -> Double
{
    guard frameDuration.isValid else
    {
        return Double(benchmarkCameraTargetFps)
    }
    let seconds = CMTimeGetSeconds(frameDuration)
    guard seconds.isFinite, seconds > 0 else
    {
        return Double(benchmarkCameraTargetFps)
    }
    return 1.0 / seconds
}

private func benchmarkCameraFormatRank(_ pixelFormat: FourCharCode) -> Int?
{
    switch pixelFormat
    {
    case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange:
        return 0
    case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
        return 1
    default:
        return nil
    }
}

private func benchmarkCameraFormatSupportsFps(
    _ format: AVCaptureDevice.Format,
    fps: Int32 = benchmarkCameraTargetFps
) -> Bool
{
    let desired = CMTimeMake(value: 1, timescale: fps)
    return format.videoSupportedFrameRateRanges.contains
    {
        range in
        CMTimeCompare(desired, range.minFrameDuration) >= 0 &&
            CMTimeCompare(desired, range.maxFrameDuration) <= 0
    }
}

private func preferredBenchmarkCameraFormat(
    for device: AVCaptureDevice
) -> AVCaptureDevice.Format?
{
    var best: AVCaptureDevice.Format?
    var bestHeightDiff = Int.max
    var bestRangeRank = Int.max
    var bestWidthDiff = Int.max
    for format in device.formats
    {
        let description = format.formatDescription
        let pixelFormat = CMFormatDescriptionGetMediaSubType(description)
        guard let rangeRank = benchmarkCameraFormatRank(pixelFormat),
              benchmarkCameraFormatSupportsFps(format)
        else
        {
            continue
        }
        let dimensions = CMVideoFormatDescriptionGetDimensions(description)
        let heightDiff = abs(Int(dimensions.height) - Int(benchmarkCameraTargetHeight))
        let widthDiff = abs(Int(dimensions.width) - Int(benchmarkCameraTargetWidth))
        let isBetter =
            heightDiff < bestHeightDiff ||
            (heightDiff == bestHeightDiff && rangeRank < bestRangeRank) ||
            (heightDiff == bestHeightDiff && rangeRank == bestRangeRank && widthDiff < bestWidthDiff)
        if isBetter
        {
            best = format
            bestHeightDiff = heightDiff
            bestRangeRank = rangeRank
            bestWidthDiff = widthDiff
        }
    }
    return best
}

func cameraStageMeasurementEnabled(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Bool
{
    guard environment[parkedCaseEnv].map({ !$0.isEmpty }) == true ||
          realAppCameraBenchmarkEnabled(environment: environment)
    else
    {
        return false
    }
    guard let raw = environment[perfCameraStageMeasurementEnv]?
        .trimmingCharacters(in: .whitespacesAndNewlines),
        !raw.isEmpty
    else
    {
        return true
    }
    return raw != "0"
}

@MainActor
private var lastBenchmarkBuildFailure: String?

func resolvePerfIterationOverride(
    env: String,
    minimum: Int,
    defaultValue: Int,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Int
{
    guard let raw = environment[env]?.trimmingCharacters(in: .whitespacesAndNewlines),
          let parsed = Int(raw)
    else
    {
        return defaultValue
    }
    return max(parsed, minimum)
}

func resolvePerfMeasureIterations(
    defaultValue: Int = 10,
    minimum: Int = 3,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Int
{
    resolvePerfIterationOverride(
        env: perfMeasureIterationsEnv,
        minimum: minimum,
        defaultValue: defaultValue,
        environment: environment
    )
}

func resolvePerfBenchmarkIterations(
    defaultValue: Int,
    minimum: Int = 12,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Int
{
    resolvePerfIterationOverride(
        env: perfBenchmarkIterationsEnv,
        minimum: minimum,
        defaultValue: defaultValue,
        environment: environment
    )
}

func resolvePerfTraceSettleSeconds(
    defaultMilliseconds: Int = 1000,
    minimumMilliseconds: Int = 0,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> TimeInterval
{
    let milliseconds = resolvePerfIterationOverride(
        env: perfTraceSettleMsEnv,
        minimum: minimumMilliseconds,
        defaultValue: defaultMilliseconds,
        environment: environment
    )
    return TimeInterval(milliseconds) / 1000.0
}

func cameraTracePhasesEnabled(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Bool
{
    environment[perfCameraTracePhasesEnv] == "1"
}

func resolveDirectPreviewMaximumDrawableCount(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Int
{
    guard let raw = environment[perfCameraMaxDrawableCountEnv],
          let value = Int(raw.trimmingCharacters(in: .whitespacesAndNewlines)) else
    {
        return 3
    }
    return min(max(value, 2), 3)
}

func resolveDirectPreviewSurfaceScale(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> CGFloat
{
    guard let raw = environment[perfCameraPreviewSurfaceScaleEnv]?
        .trimmingCharacters(in: .whitespacesAndNewlines),
        let value = Double(raw)
    else
    {
        return 1.0
    }
    if !value.isFinite || value <= 0
    {
        return 1.0
    }
    return CGFloat(min(max(value, 0.25), 1.0))
}

func resolveCameraCaptureContractMode(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> OxideCameraCaptureContractMode
{
    let raw = environment[perfCameraCaptureContractModeEnv]?
        .trimmingCharacters(in: .whitespacesAndNewlines)
        .lowercased()
    switch raw
    {
    case OxideCameraCaptureContractMode.preset720p.rawValue, "preset720p", "hd1280x720":
        return .preset720p
    default:
        return .inputPriority
    }
}

func realAppCameraBenchmarkEnabled(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Bool
{
    environment[perfCameraRealAppHostEnv] == "1"
}

func realAppCameraBenchmarkUsesHybridVisiblePreview(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Bool
{
    environment[perfCameraRealAppHybridVisiblePreviewEnv] == "1"
}

@MainActor
func configureDirectPreviewMetalLayer(
    view: UIView,
    layer: CAMetalLayer,
    environment: [String: String] = ProcessInfo.processInfo.environment
)
{
    view.isOpaque = true
    layer.isOpaque = true
    layer.framebufferOnly = true
    if #available(iOS 11.2, *)
    {
        layer.maximumDrawableCount = resolveDirectPreviewMaximumDrawableCount(environment: environment)
    }
}

@MainActor
private func recordBenchmarkBuildFailure(_ message: String)
{
    lastBenchmarkBuildFailure = message
    emitConsoleLine("OXIDE_BENCHMARK_BUILD_FAIL \(message)")
}

@MainActor
func takeBenchmarkBuildFailure() -> String?
{
    defer
    {
        lastBenchmarkBuildFailure = nil
    }
    return lastBenchmarkBuildFailure
}

private func withPerfSignpost<T>(_ name: StaticString, body: () -> T) -> T
{
    let signpostID = OSSignpostID(log: perfSignpostLog)
    os_signpost(.begin, log: perfSignpostLog, name: name, signpostID: signpostID)
    let result = body()
    os_signpost(.end, log: perfSignpostLog, name: name, signpostID: signpostID)
    return result
}

var perfConsoleLineEmitterOverride: ((String) -> Void)?

func emitConsoleLine(_ line: String)
{
    if let emitter = perfConsoleLineEmitterOverride
    {
        emitter(line)
        return
    }
    guard let data = "\(line)\n".data(using: .utf8) else
    {
        return
    }
    try? FileHandle.standardOutput.write(contentsOf: data)
}

func postDarwinNotification(_ name: String)
{
    CFNotificationCenterPostNotification(
        CFNotificationCenterGetDarwinNotifyCenter(),
        CFNotificationName(name as CFString),
        nil,
        nil,
        true
    )
}

@MainActor
func collectOxidePerfRunnerJSON(smoke: Bool) -> String?
{
    oxideHostClearPerfReportJSON()
    guard oxideHostRunPerfSuite(smoke ? 1 : 0) == 0 else
    {
        recordBenchmarkBuildFailure("failed - oxide perf suite execution returned non-zero")
        return nil
    }
    let needed = oxideHostPerfReportJSONLen()
    guard needed > 1 else
    {
        recordBenchmarkBuildFailure("failed - oxide perf suite produced an empty JSON payload")
        return nil
    }
    let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: needed)
    defer
    {
        buffer.deallocate()
        oxideHostClearPerfReportJSON()
    }
    let copied = oxideHostCopyPerfReportJSON(buffer, needed)
    guard copied == needed else
    {
        recordBenchmarkBuildFailure(
            "failed - oxide perf suite JSON copy returned \(copied) bytes, expected \(needed)"
        )
        return nil
    }
    let data = Data(bytes: buffer, count: needed - 1)
    guard let json = String(data: data, encoding: .utf8) else
    {
        recordBenchmarkBuildFailure("failed - oxide perf suite JSON payload was not valid UTF-8")
        return nil
    }
    return json
}

func emitOxidePerfRunnerJSON(_ json: String)
{
    emitConsoleLine(oxidePerfReportBeginLine)
    let payload = Data(json.utf8).base64EncodedString()
    let chunkSize = 3072
    var start = payload.startIndex
    while start < payload.endIndex
    {
        let end = payload.index(start, offsetBy: chunkSize, limitedBy: payload.endIndex) ?? payload.endIndex
        emitConsoleLine("\(oxidePerfReportChunkPrefix)\(payload[start..<end])")
        start = end
    }
    emitConsoleLine(oxidePerfReportEndLine)
}

@_silgen_name("oxide_host_app_init")
private func oxideHostAppInit(_ width: UInt32, _ height: UInt32, _ scale: Float) -> Int32

@_silgen_name("oxide_host_app_frame")
private func oxideHostAppFrame(_ width: UInt32, _ height: UInt32, _ scale: Float) -> Int32

@_silgen_name("oxide_host_app_frame_with_drawable")
private func oxideHostAppFrameWithDrawable(
    _ width: UInt32,
    _ height: UInt32,
    _ scale: Float,
    _ drawable: UnsafeMutableRawPointer?
) -> Int32

@_silgen_name("oxide_host_camera_preview_plan")
private func oxideHostCameraPreviewPlan(
    _ width: UInt32,
    _ height: UInt32,
    _ scale: Float
) -> Int32

@_silgen_name("oxide_host_camera_preview_plan_reason")
private func oxideHostCameraPreviewPlanReason(
    _ width: UInt32,
    _ height: UInt32,
    _ scale: Float
) -> Int32

@_silgen_name("oxide_host_app_stats")
private func oxideHostAppStats(_ out: UnsafeMutablePointer<OxideHostStats>?) -> Int32

@_silgen_name("oxide_host_camera_tick_perf")
private func oxideHostCameraTickPerf(_ out: UnsafeMutablePointer<OxideHostCameraTickPerf>?) -> Int32

@_silgen_name("oxide_host_app_debug_perf")
private func oxideHostAppDebugPerf(_ out: UnsafeMutablePointer<OxideHostAppDebugPerf>?) -> Int32

@_silgen_name("oxide_host_app_shutdown")
private func oxideHostAppShutdown()

@_silgen_name("oxide_host_set_benchmark_mode")
private func oxideHostSetBenchmarkMode(_ on: UInt8) -> Int32

@_silgen_name("oxide_host_scene_count")
private func oxideHostSceneCount() -> UInt32

@_silgen_name("oxide_host_scene_name")
private func oxideHostSceneName(
    _ index: UInt32,
    _ outPtr: UnsafeMutablePointer<CChar>?,
    _ outLen: UInt32
) -> UInt32

@_silgen_name("oxide_host_set_scene")
private func oxideHostSetScene(_ index: UInt32) -> Int32

@_silgen_name("oxide_host_set_camera_options")
private func oxideHostSetCameraOptions(
    _ blur: UInt8,
    _ sigma: Float,
    _ grayscale: UInt8,
    _ animate: UInt8
) -> Int32

@_silgen_name("oxide_host_set_camera_running")
private func oxideHostSetCameraRunning(_ on: UInt8) -> Int32

@_silgen_name("oxide_host_reset_camera_perf_counters")
private func oxideHostResetCameraPerfCounters() -> Int32

@_silgen_name("oxide_host_set_camera_running_mode")
private func oxideHostSetCameraRunningMode(_ on: UInt8, _ previewOnly: UInt8) -> Int32

@_silgen_name("oxide_host_set_camera_render_mode")
private func oxideHostSetCameraRenderMode(_ mode: Int32) -> Int32

@_silgen_name("oxide_host_set_camera_texture_source")
private func oxideHostSetCameraTextureSource(_ source: Int32) -> Int32

@_silgen_name("oxide_cam_get_running_session")
private func oxideCamGetRunningSession() -> UnsafeMutableRawPointer?

@_silgen_name("oxide_host_run_perf_suite")
private func oxideHostRunPerfSuite(_ smoke: UInt8) -> Int32

@_silgen_name("oxide_host_perf_report_json_len")
private func oxideHostPerfReportJSONLen() -> Int

@_silgen_name("oxide_host_copy_perf_report_json")
private func oxideHostCopyPerfReportJSON(
    _ outPtr: UnsafeMutablePointer<UInt8>?,
    _ outLen: Int
) -> Int

@_silgen_name("oxide_host_clear_perf_report_json")
private func oxideHostClearPerfReportJSON()

private struct OxideHostStats
{
    var fps: Float = 0
    var draws: UInt32 = 0
    var anims: UInt32 = 0
    var memoryWarnings: UInt32 = 0
    var damagePct: Float = 0
    var damageRects: UInt32 = 0
    var camCoveragePct: Float = 0
    var camBlurMs: Float = 0
    var camBlurUpdates: UInt32 = 0
    var camUpdatePeriodMs: UInt32 = 0
    var camPaused: UInt8 = 0
    var camLowPower: UInt8 = 0
    var camThermal: UInt8 = 0
    var camWidth: UInt32 = 0
    var camHeight: UInt32 = 0
    var camBitDepth: UInt8 = 0
    var camMatrix: UInt8 = 0
    var camVideoRange: UInt8 = 0
    var camColorSpace: UInt8 = 0
    var camRunning: UInt8 = 0
    var camFps: Float = 0
    var camPollSubmissionsMs: Float = 0
    var camFetchMs: Float = 0
    var camSetupMs: Float = 0
    var camEncodeQuadMs: Float = 0
    var camCommandBufferMs: Float = 0
    var camEncoderMs: Float = 0
    var camEncodeBindMs: Float = 0
    var camEncodeDrawMs: Float = 0
    var camEndEncodingMs: Float = 0
    var camPresentMs: Float = 0
    var camCommitMs: Float = 0
    var camGpuMs: Float = 0
    var camCaptureTotalMs: Float = 0
    var camCaptureSampleSetupMs: Float = 0
    var camCaptureLockMs: Float = 0
    var camCaptureTextureBridgeMs: Float = 0
    var camCapturePublishMs: Float = 0
    var camCapturePublishLockMs: Float = 0
    var camCapturePublishTextureRefsMs: Float = 0
    var camCapturePublishPixelBufferMs: Float = 0
    var camCaptureFrameDeliveryMs: Float = 0
    var camSampleDeliveryPoolBytes: UInt64 = 0
    var camSampleDeliveryPoolSurfaces: UInt32 = 0
    var camActiveSampleSurfaceBytes: UInt64 = 0
    var camActiveSampleSurfaceSurfaces: UInt32 = 0
    var camActiveSampleBuffers: UInt32 = 0
    var camPeakActiveSampleSurfaceBytes: UInt64 = 0
    var camPeakActiveSampleSurfaceSurfaces: UInt32 = 0
    var camPeakActiveSampleBuffers: UInt32 = 0
    var camSampleDeliveryTotalSamples: UInt32 = 0
    var camSampleDeliveryReusedFrames: UInt32 = 0
    var camSampleDeliveryReusedSurfaces: UInt32 = 0
    var camSampleDeliveryMaxReuseGapFrames: UInt32 = 0
    var camRetainedSampleSurfaceBytes: UInt64 = 0
    var camRetainedSampleSurfaceSurfaces: UInt32 = 0
    var camRetainedPublishedSlotSurfaceBytes: UInt64 = 0
    var camRetainedPublishedSlotSurfaces: UInt32 = 0
    var camRetainedLatestPixelBufferSurfaceBytes: UInt64 = 0
    var camRetainedLatestPixelBufferSurfaceSurfaces: UInt32 = 0
    var camLatestPublishedGeneration: UInt64 = 0
    var camLatestPublishedTimestampNs: UInt64 = 0
    var rendererMemoryTotalBytes: UInt64 = 0
    var rendererMemoryDrawTargetsBytes: UInt64 = 0
    var rendererMemoryDrawTargetMainBytes: UInt64 = 0
    var rendererMemoryDrawTargetMsaaBytes: UInt64 = 0
    var rendererMemoryEffectTargetsBytes: UInt64 = 0
    var rendererMemoryEffectPrepassBytes: UInt64 = 0
    var rendererMemoryEffectBlurChainBytes: UInt64 = 0
    var rendererMemoryLiveCameraBytes: UInt64 = 0
    var rendererMemoryCameraCacheBytes: UInt64 = 0
    var rendererMemoryCameraBlurCacheBytes: UInt64 = 0
    var rendererMemoryCameraTransitionCacheBytes: UInt64 = 0
    var rendererMemoryBenchmarkCameraBytes: UInt64 = 0
    var rendererMemoryLayerCacheBytes: UInt64 = 0
    var rendererMemoryImageCacheBytes: UInt64 = 0
    var rendererMemoryBufferBytes: UInt64 = 0
    var rendererPendingCommandBuffers: UInt32 = 0
    var rendererPendingPresentDrawables: UInt32 = 0
    var rendererPendingPresentTextures: UInt32 = 0
    var rendererPreviewSubmissionDepth: UInt32 = 0
    var rendererPreviewSubmissionSkipped: UInt32 = 0
    var rendererPreviewSubmissionFrameAgeMs: Float = 0
}

private struct OxideHostCameraTickPerf
{
    var serial: UInt64 = 0
    var drawableWidth: UInt32 = 0
    var drawableHeight: UInt32 = 0
    var drawableScale: Float = 0
    var planReason: UInt32 = 0
    var planMs: Float = 0
    var drawableAcquireMs: Float = 0
    var frameCallMs: Float = 0
    var tickTotalMs: Float = 0
    var skipped: UInt8 = 0
    var drawableAcquired: UInt8 = 0
    var frameSubmitted: UInt8 = 0
    var reserved: UInt8 = 0
}

private struct OxideHostAppDebugPerf
{
    var sceneWillConnectCalls: UInt32 = 0
    var perfSceneBranchCalls: UInt32 = 0
    var normalSceneBranchCalls: UInt32 = 0
    var metalViewInstalls: UInt32 = 0
    var displayLinkCreateCalls: UInt32 = 0
    var sceneDidBecomeActiveCalls: UInt32 = 0
    var sceneWillEnterForegroundCalls: UInt32 = 0
    var ensureHostInitializedCalls: UInt32 = 0
    var hostReadyTransitions: UInt32 = 0
    var onTickCalls: UInt32 = 0
    var runningUiTest: UInt8 = 0
    var runningPerfBenchmarkHost: UInt8 = 0
    var shouldRender: UInt8 = 0
    var hostReady: UInt8 = 0
}

private struct OxideStageMetricSummary: Codable
{
    let unit: String
    let min: Double
    let max: Double
    let mean: Double
    let median: Double
    let p95: Double
    let p99: Double
    let samples: Int
}

private struct OxideStageSummaryPayload: Codable
{
    let stages: [String: OxideStageMetricSummary]
}

private struct OxideMemorySummaryPayload: Codable
{
    let categories: [String: OxideStageMetricSummary]
}

private struct OxidePreviewPlanSummaryPayload: Codable
{
    let counts: [String: Int]
}

private struct OxideTickDebugSummaryPayload: Codable
{
    let startSerial: UInt64
    let lastObservedSerial: UInt64
    let maxObservedSerial: UInt64
    let polls: Int
    let tickReadFailures: Int
    let stalePolls: Int
    let newSerials: Int
    let statsReadFailures: Int
    let recordedTicks: Int
    let skippedTicks: Int
    let drawableAcquiredTicks: Int
    let frameSubmittedTicks: Int
}

private struct OxideAppHostDebugSummaryPayload: Codable
{
    let sceneWillConnectCalls: UInt32
    let perfSceneBranchCalls: UInt32
    let normalSceneBranchCalls: UInt32
    let metalViewInstalls: UInt32
    let displayLinkCreateCalls: UInt32
    let sceneDidBecomeActiveCalls: UInt32
    let sceneWillEnterForegroundCalls: UInt32
    let ensureHostInitializedCalls: UInt32
    let hostReadyTransitions: UInt32
    let onTickCalls: UInt32
    let runningUiTest: Bool
    let runningPerfBenchmarkHost: Bool
    let shouldRender: Bool
    let hostReady: Bool
}

private func perfNowMs() -> Double
{
    CACurrentMediaTime() * 1000.0
}

private func oxideStagePercentile(_ sortedValues: [Double], percentile: Double) -> Double
{
    guard !sortedValues.isEmpty else
    {
        return 0
    }
    let clamped = min(max(percentile, 0), 1)
    let index = Int((Double(sortedValues.count - 1) * clamped).rounded())
    return sortedValues[min(max(index, 0), sortedValues.count - 1)]
}

private func summarizeStageSamples(_ values: [Double]) -> OxideStageMetricSummary?
{
    let filtered = values.filter { $0.isFinite }
    guard !filtered.isEmpty else
    {
        return nil
    }
    let sorted = filtered.sorted()
    let sum = sorted.reduce(0, +)
    return OxideStageMetricSummary(
        unit: "ms",
        min: sorted.first ?? 0,
        max: sorted.last ?? 0,
        mean: sum / Double(sorted.count),
        median: oxideStagePercentile(sorted, percentile: 0.50),
        p95: oxideStagePercentile(sorted, percentile: 0.95),
        p99: oxideStagePercentile(sorted, percentile: 0.99),
        samples: sorted.count
    )
}

private final class OxideCameraStageAccumulator
{
    private static let orderedStageNames = [
        "camera.host.plan",
        "camera.drawable.acquire",
        "camera.host.frame",
        "camera.host.tick_total",
        "camera.renderer.direct.poll_submissions",
        "camera.renderer.direct.fetch",
        "camera.renderer.direct.setup",
        "camera.renderer.direct.command_buffer",
        "camera.renderer.direct.encoder",
        "camera.renderer.direct.encode_quad",
        "camera.renderer.direct.encode.bind",
        "camera.renderer.direct.encode.draw",
        "camera.renderer.direct.end_encoding",
        "camera.renderer.direct.present_drawable",
        "camera.renderer.direct.commit",
        "camera.renderer.direct.gpu_total",
        "camera.capture.total",
        "camera.capture.sample_setup",
        "camera.capture.lock",
        "camera.capture.texture_bridge",
        "camera.capture.publish",
        "camera.capture.publish.lock",
        "camera.capture.publish.texture_refs",
        "camera.capture.publish.pixel_buffer",
        "camera.capture.frame_delivery",
    ]

    private var valuesByStage: [String: [Double]] = [:]

    init()
    {
        reset()
    }

    func reset()
    {
        valuesByStage.removeAll(keepingCapacity: true)
        for stageName in Self.orderedStageNames
        {
            valuesByStage[stageName] = []
        }
    }

    func record(
        hostPlanMs: Double,
        drawableAcquireMs: Double,
        hostFrameMs: Double,
        hostTickTotalMs: Double,
        stats: OxideHostStats
    )
    {
        append(hostPlanMs, for: "camera.host.plan")
        append(drawableAcquireMs, for: "camera.drawable.acquire")
        append(hostFrameMs, for: "camera.host.frame")
        append(hostTickTotalMs, for: "camera.host.tick_total")
        append(Double(stats.camPollSubmissionsMs), for: "camera.renderer.direct.poll_submissions")
        append(Double(stats.camFetchMs), for: "camera.renderer.direct.fetch")
        append(Double(stats.camSetupMs), for: "camera.renderer.direct.setup")
        append(Double(stats.camCommandBufferMs), for: "camera.renderer.direct.command_buffer")
        append(Double(stats.camEncoderMs), for: "camera.renderer.direct.encoder")
        append(Double(stats.camEncodeQuadMs), for: "camera.renderer.direct.encode_quad")
        append(Double(stats.camEncodeBindMs), for: "camera.renderer.direct.encode.bind")
        append(Double(stats.camEncodeDrawMs), for: "camera.renderer.direct.encode.draw")
        append(Double(stats.camEndEncodingMs), for: "camera.renderer.direct.end_encoding")
        append(Double(stats.camPresentMs), for: "camera.renderer.direct.present_drawable")
        append(Double(stats.camCommitMs), for: "camera.renderer.direct.commit")
        append(Double(stats.camGpuMs), for: "camera.renderer.direct.gpu_total")
        append(Double(stats.camCaptureTotalMs), for: "camera.capture.total")
        append(Double(stats.camCaptureSampleSetupMs), for: "camera.capture.sample_setup")
        append(Double(stats.camCaptureLockMs), for: "camera.capture.lock")
        append(Double(stats.camCaptureTextureBridgeMs), for: "camera.capture.texture_bridge")
        append(Double(stats.camCapturePublishMs), for: "camera.capture.publish")
        append(Double(stats.camCapturePublishLockMs), for: "camera.capture.publish.lock")
        append(Double(stats.camCapturePublishTextureRefsMs), for: "camera.capture.publish.texture_refs")
        append(Double(stats.camCapturePublishPixelBufferMs), for: "camera.capture.publish.pixel_buffer")
        append(Double(stats.camCaptureFrameDeliveryMs), for: "camera.capture.frame_delivery")
    }

    func recordSkippedFrame()
    {
        for stageName in Self.orderedStageNames
        {
            append(0.0, for: stageName)
        }
    }

    func summaryLine() -> String?
    {
        var stages: [String: OxideStageMetricSummary] = [:]
        for stageName in Self.orderedStageNames
        {
            if let values = valuesByStage[stageName],
               let summary = summarizeStageSamples(values)
            {
                stages[stageName] = summary
            }
        }
        guard !stages.isEmpty else
        {
            return nil
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(OxideStageSummaryPayload(stages: stages)),
              let json = String(data: data, encoding: .utf8) else
        {
            return nil
        }
        return "\(oxideStageSummaryPrefix)\(json)"
    }

    private func append(_ value: Double, for stageName: String)
    {
        let clamped = max(value, 0)
        valuesByStage[stageName, default: []].append(clamped)
    }
}

private final class OxideCameraPreviewPlanAccumulator
{
    private var countsByReason: [String: Int] = [:]

    func reset()
    {
        countsByReason.removeAll(keepingCapacity: true)
    }

    func record(reason: Int32)
    {
        let labels = labelsForReason(reason)
        guard !labels.isEmpty else
        {
            return
        }
        for label in labels
        {
            countsByReason[label, default: 0] += 1
        }
    }

    func summaryLine() -> String?
    {
        guard !countsByReason.isEmpty else
        {
            return nil
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(OxidePreviewPlanSummaryPayload(counts: countsByReason)),
              let json = String(data: data, encoding: .utf8) else
        {
            return nil
        }
        return "\(oxidePreviewPlanSummaryPrefix)\(json)"
    }

    private func labelsForReason(_ reason: Int32) -> [String]
    {
        if reason < 0
        {
            return ["host_error_\(reason)"]
        }
        if reason == 0
        {
            return ["skip"]
        }
        let flagLabels: [(Int32, String)] = [
            (1 << 0, "submit_error"),
            (1 << 1, "non_direct_preview"),
            (1 << 2, "resize"),
            (1 << 3, "camera_stopped"),
            (1 << 4, "non_live_source"),
            (1 << 5, "non_nv12_mode"),
            (1 << 6, "no_current_frame"),
            (1 << 7, "new_timestamp"),
            (1 << 8, "new_generation"),
            (1 << 9, "backpressure"),
        ]
        let labels = flagLabels.compactMap
        {
            (flag, label) in
            (reason & flag) != 0 ? label : nil
        }
        if !labels.isEmpty
        {
            return labels
        }
        return ["unknown_\(reason)"]
    }
}

private func encodeTickDebugSummaryLine(
    startSerial: UInt64,
    lastObservedSerial: UInt64,
    maxObservedSerial: UInt64,
    polls: Int,
    tickReadFailures: Int,
    stalePolls: Int,
    newSerials: Int,
    statsReadFailures: Int,
    recordedTicks: Int,
    skippedTicks: Int,
    drawableAcquiredTicks: Int,
    frameSubmittedTicks: Int
) -> String?
{
    let payload = OxideTickDebugSummaryPayload(
        startSerial: startSerial,
        lastObservedSerial: lastObservedSerial,
        maxObservedSerial: maxObservedSerial,
        polls: polls,
        tickReadFailures: tickReadFailures,
        stalePolls: stalePolls,
        newSerials: newSerials,
        statsReadFailures: statsReadFailures,
        recordedTicks: recordedTicks,
        skippedTicks: skippedTicks,
        drawableAcquiredTicks: drawableAcquiredTicks,
        frameSubmittedTicks: frameSubmittedTicks
    )
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    guard let data = try? encoder.encode(payload),
          let json = String(data: data, encoding: .utf8) else
    {
        return nil
    }
    return "\(oxideTickDebugSummaryPrefix)\(json)"
}

private func encodeAppHostDebugSummaryLine(
    _ payload: OxideAppHostDebugSummaryPayload
) -> String?
{
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    guard let data = try? encoder.encode(payload),
          let json = String(data: data, encoding: .utf8) else
    {
        return nil
    }
    return "\(oxideAppHostDebugSummaryPrefix)\(json)"
}

private final class OxideCameraMemoryAccumulator
{
    private static let orderedCategories: [(name: String, unit: String)] = [
        ("camera.sample_delivery_pool_bytes_est", "bytes"),
        ("camera.sample_delivery_pool_surfaces", "count"),
        ("camera.active_sample_surface_bytes_est", "bytes"),
        ("camera.active_sample_surface_surfaces", "count"),
        ("camera.active_sample_buffers", "count"),
        ("camera.peak_active_sample_surface_bytes_est", "bytes"),
        ("camera.peak_active_sample_surface_surfaces", "count"),
        ("camera.peak_active_sample_buffers", "count"),
        ("camera.sample_delivery_total_samples", "count"),
        ("camera.sample_delivery_reused_frames", "count"),
        ("camera.sample_delivery_reused_surfaces", "count"),
        ("camera.sample_delivery_max_reuse_gap_frames", "count"),
        ("camera.sample_delivery_reuse_fraction", "fraction"),
        ("camera.retained_sample_surface_bytes_est", "bytes"),
        ("camera.retained_sample_surface_surfaces", "count"),
        ("camera.retained_published_slot_surface_bytes_est", "bytes"),
        ("camera.retained_published_slot_surfaces", "count"),
        ("camera.retained_latest_pixel_buffer_surface_bytes_est", "bytes"),
        ("camera.retained_latest_pixel_buffer_surface_surfaces", "count"),
        ("renderer.total_bytes", "bytes"),
        ("renderer.draw_targets_bytes", "bytes"),
        ("renderer.draw_target_main_bytes", "bytes"),
        ("renderer.draw_target_msaa_bytes", "bytes"),
        ("renderer.effect_targets_bytes", "bytes"),
        ("renderer.effect_prepass_bytes", "bytes"),
        ("renderer.effect_blur_chain_bytes", "bytes"),
        ("renderer.live_camera_bytes", "bytes"),
        ("renderer.camera_cache_bytes", "bytes"),
        ("renderer.camera_blur_cache_bytes", "bytes"),
        ("renderer.camera_transition_cache_bytes", "bytes"),
        ("renderer.benchmark_camera_bytes", "bytes"),
        ("renderer.layer_cache_bytes", "bytes"),
        ("renderer.image_cache_bytes", "bytes"),
        ("renderer.buffer_bytes", "bytes"),
        ("renderer.pending_command_buffers", "count"),
        ("renderer.pending_present_drawables", "count"),
        ("renderer.pending_present_textures", "count"),
        ("renderer.preview_submission_depth", "count"),
        ("renderer.preview_submission_depth_is_0", "fraction"),
        ("renderer.preview_submission_depth_is_1", "fraction"),
        ("renderer.preview_submission_depth_is_2_or_more", "fraction"),
        ("renderer.preview_submission_skipped", "fraction"),
        ("renderer.preview_submission_frame_age_ms", "ms"),
        ("view.drawable_single_bytes_est", "bytes"),
        ("view.drawable_pool_bytes_est", "bytes"),
        ("known.total_bytes_est", "bytes"),
    ]

    private var valuesByCategory: [String: [Double]] = [:]

    init()
    {
        reset()
    }

    func reset()
    {
        valuesByCategory.removeAll(keepingCapacity: true)
        for category in Self.orderedCategories
        {
            valuesByCategory[category.name] = []
        }
    }

    func record(
        stats: OxideHostStats,
        drawableWidth: UInt32,
        drawableHeight: UInt32,
        layer: CAMetalLayer
    )
    {
        record(
            stats: stats,
            drawableWidth: drawableWidth,
            drawableHeight: drawableHeight,
            pixelFormat: layer.pixelFormat,
            maximumDrawableCount: max(layer.maximumDrawableCount, 1)
        )
    }

    func record(
        stats: OxideHostStats,
        drawableWidth: UInt32,
        drawableHeight: UInt32,
        pixelFormat: MTLPixelFormat,
        maximumDrawableCount: Int
    )
    {
        append(Double(stats.camSampleDeliveryPoolBytes), for: "camera.sample_delivery_pool_bytes_est")
        append(Double(stats.camSampleDeliveryPoolSurfaces), for: "camera.sample_delivery_pool_surfaces")
        append(Double(stats.camActiveSampleSurfaceBytes), for: "camera.active_sample_surface_bytes_est")
        append(
            Double(stats.camActiveSampleSurfaceSurfaces),
            for: "camera.active_sample_surface_surfaces"
        )
        append(Double(stats.camActiveSampleBuffers), for: "camera.active_sample_buffers")
        append(
            Double(stats.camPeakActiveSampleSurfaceBytes),
            for: "camera.peak_active_sample_surface_bytes_est"
        )
        append(
            Double(stats.camPeakActiveSampleSurfaceSurfaces),
            for: "camera.peak_active_sample_surface_surfaces"
        )
        append(Double(stats.camPeakActiveSampleBuffers), for: "camera.peak_active_sample_buffers")
        append(Double(stats.camSampleDeliveryTotalSamples), for: "camera.sample_delivery_total_samples")
        append(Double(stats.camSampleDeliveryReusedFrames), for: "camera.sample_delivery_reused_frames")
        append(
            Double(stats.camSampleDeliveryReusedSurfaces),
            for: "camera.sample_delivery_reused_surfaces"
        )
        append(
            Double(stats.camSampleDeliveryMaxReuseGapFrames),
            for: "camera.sample_delivery_max_reuse_gap_frames"
        )
        let sampleReuseFraction =
            stats.camSampleDeliveryTotalSamples > 0
            ? Double(stats.camSampleDeliveryReusedFrames) /
                Double(stats.camSampleDeliveryTotalSamples)
            : 0.0
        append(sampleReuseFraction, for: "camera.sample_delivery_reuse_fraction")
        append(
            Double(stats.camRetainedSampleSurfaceBytes),
            for: "camera.retained_sample_surface_bytes_est"
        )
        append(
            Double(stats.camRetainedSampleSurfaceSurfaces),
            for: "camera.retained_sample_surface_surfaces"
        )
        append(
            Double(stats.camRetainedPublishedSlotSurfaceBytes),
            for: "camera.retained_published_slot_surface_bytes_est"
        )
        append(
            Double(stats.camRetainedPublishedSlotSurfaces),
            for: "camera.retained_published_slot_surfaces"
        )
        append(
            Double(stats.camRetainedLatestPixelBufferSurfaceBytes),
            for: "camera.retained_latest_pixel_buffer_surface_bytes_est"
        )
        append(
            Double(stats.camRetainedLatestPixelBufferSurfaceSurfaces),
            for: "camera.retained_latest_pixel_buffer_surface_surfaces"
        )
        append(Double(stats.rendererMemoryTotalBytes), for: "renderer.total_bytes")
        append(Double(stats.rendererMemoryDrawTargetsBytes), for: "renderer.draw_targets_bytes")
        append(Double(stats.rendererMemoryDrawTargetMainBytes), for: "renderer.draw_target_main_bytes")
        append(Double(stats.rendererMemoryDrawTargetMsaaBytes), for: "renderer.draw_target_msaa_bytes")
        append(Double(stats.rendererMemoryEffectTargetsBytes), for: "renderer.effect_targets_bytes")
        append(Double(stats.rendererMemoryEffectPrepassBytes), for: "renderer.effect_prepass_bytes")
        append(
            Double(stats.rendererMemoryEffectBlurChainBytes),
            for: "renderer.effect_blur_chain_bytes"
        )
        append(Double(stats.rendererMemoryLiveCameraBytes), for: "renderer.live_camera_bytes")
        append(Double(stats.rendererMemoryCameraCacheBytes), for: "renderer.camera_cache_bytes")
        append(
            Double(stats.rendererMemoryCameraBlurCacheBytes),
            for: "renderer.camera_blur_cache_bytes"
        )
        append(
            Double(stats.rendererMemoryCameraTransitionCacheBytes),
            for: "renderer.camera_transition_cache_bytes"
        )
        append(Double(stats.rendererMemoryBenchmarkCameraBytes), for: "renderer.benchmark_camera_bytes")
        append(Double(stats.rendererMemoryLayerCacheBytes), for: "renderer.layer_cache_bytes")
        append(Double(stats.rendererMemoryImageCacheBytes), for: "renderer.image_cache_bytes")
        append(Double(stats.rendererMemoryBufferBytes), for: "renderer.buffer_bytes")
        append(Double(stats.rendererPendingCommandBuffers), for: "renderer.pending_command_buffers")
        append(
            Double(stats.rendererPendingPresentDrawables),
            for: "renderer.pending_present_drawables"
        )
        append(
            Double(stats.rendererPendingPresentTextures),
            for: "renderer.pending_present_textures"
        )
        let previewDepth = Double(stats.rendererPreviewSubmissionDepth)
        append(previewDepth, for: "renderer.preview_submission_depth")
        append(previewDepth == 0 ? 1.0 : 0.0, for: "renderer.preview_submission_depth_is_0")
        append(previewDepth == 1 ? 1.0 : 0.0, for: "renderer.preview_submission_depth_is_1")
        append(
            previewDepth >= 2 ? 1.0 : 0.0,
            for: "renderer.preview_submission_depth_is_2_or_more"
        )
        append(
            stats.rendererPreviewSubmissionSkipped == 0 ? 0.0 : 1.0,
            for: "renderer.preview_submission_skipped"
        )
        if stats.rendererPreviewSubmissionFrameAgeMs > 0
        {
            append(
                Double(stats.rendererPreviewSubmissionFrameAgeMs),
                for: "renderer.preview_submission_frame_age_ms"
            )
        }

        let drawableSingleBytes = estimatedDrawableBytes(
            width: drawableWidth,
            height: drawableHeight,
            pixelFormat: pixelFormat
        )
        let drawablePoolBytes =
            saturatingMultiply(drawableSingleBytes, UInt64(max(maximumDrawableCount, 1)))
        append(Double(drawableSingleBytes), for: "view.drawable_single_bytes_est")
        append(Double(drawablePoolBytes), for: "view.drawable_pool_bytes_est")
        let knownOwnedBytes = saturatingAdd(
            saturatingAdd(stats.rendererMemoryTotalBytes, drawablePoolBytes),
            stats.camRetainedSampleSurfaceBytes
        )
        append(
            Double(knownOwnedBytes),
            for: "known.total_bytes_est"
        )
    }

    func summaryLine() -> String?
    {
        var categories: [String: OxideStageMetricSummary] = [:]
        for category in Self.orderedCategories
        {
            guard let values = valuesByCategory[category.name] else
            {
                continue
            }
            let filtered = values.filter { $0.isFinite }
            guard !filtered.isEmpty else
            {
                continue
            }
            let sorted = filtered.sorted()
            let sum = sorted.reduce(0, +)
            categories[category.name] = OxideStageMetricSummary(
                unit: category.unit,
                min: sorted.first ?? 0,
                max: sorted.last ?? 0,
                mean: sum / Double(sorted.count),
                median: oxideStagePercentile(sorted, percentile: 0.50),
                p95: oxideStagePercentile(sorted, percentile: 0.95),
                p99: oxideStagePercentile(sorted, percentile: 0.99),
                samples: sorted.count
            )
        }
        guard !categories.isEmpty else
        {
            return nil
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(OxideMemorySummaryPayload(categories: categories)),
              let json = String(data: data, encoding: .utf8) else
        {
            return nil
        }
        return "\(oxideMemorySummaryPrefix)\(json)"
    }

    private func append(_ value: Double, for categoryName: String)
    {
        let clamped = max(value, 0)
        valuesByCategory[categoryName, default: []].append(clamped)
    }

    private func estimatedDrawableBytes(
        width: UInt32,
        height: UInt32,
        pixelFormat: MTLPixelFormat
    ) -> UInt64
    {
        saturatingMultiply(
            saturatingMultiply(UInt64(width), UInt64(height)),
            drawableBytesPerPixel(for: pixelFormat)
        )
    }

    private func drawableBytesPerPixel(for pixelFormat: MTLPixelFormat) -> UInt64
    {
        switch pixelFormat
        {
        case .rgba16Float:
            return 8
        case .bgr10_xr, .bgr10_xr_srgb:
            return 8
        default:
            return 4
        }
    }

    private func saturatingMultiply(_ lhs: UInt64, _ rhs: UInt64) -> UInt64
    {
        let (result, overflow) = lhs.multipliedReportingOverflow(by: rhs)
        return overflow ? .max : result
    }

    private func saturatingAdd(_ lhs: UInt64, _ rhs: UInt64) -> UInt64
    {
        let (result, overflow) = lhs.addingReportingOverflow(rhs)
        return overflow ? .max : result
    }
}

final class DarwinNotificationObserver
{
    private let name: String
    private let callback: () -> Void

    init(name: String, callback: @escaping () -> Void)
    {
        self.name = name
        self.callback = callback
        let rawObserver = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        CFNotificationCenterAddObserver(
            CFNotificationCenterGetDarwinNotifyCenter(),
            rawObserver,
            {
                _, observer, _, _, _ in
                guard let observer else
                {
                    return
                }
                let token = Unmanaged<DarwinNotificationObserver>
                    .fromOpaque(observer)
                    .takeUnretainedValue()
                DispatchQueue.main.async
                {
                    token.callback()
                }
            },
            name as CFString,
            nil,
            .deliverImmediately
        )
    }

    deinit
    {
        let rawObserver = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        CFNotificationCenterRemoveObserver(
            CFNotificationCenterGetDarwinNotifyCenter(),
            rawObserver,
            CFNotificationName(name as CFString),
            nil
        )
    }
}

@MainActor
final class PerfSurfaceHost
{
    let rootViewController: UIViewController
    let containerView: UIView
    private var refreshUpdateLink: UIUpdateLink?
    private weak var installedWindow: UIWindow?

    init(containerSize: CGSize = CGSize(width: 430, height: 932))
    {
        self.rootViewController = UIViewController()
        self.containerView = UIView(frame: CGRect(origin: .zero, size: containerSize))
        self.containerView.backgroundColor = .white
        self.rootViewController.view = containerView
    }

    @discardableResult
    func installInNewWindow(frame: CGRect? = nil, makeKey: Bool = true) -> UIWindow
    {
        let window: UIWindow
        if let scene = Self.preferredWindowScene()
        {
            window = UIWindow(windowScene: scene)
        }
        else
        {
            window = UIWindow(frame: frame ?? UIScreen.main.bounds)
        }
        attach(to: window, makeKey: makeKey)
        return window
    }

    func attach(to window: UIWindow, makeKey: Bool = true)
    {
        installedWindow = window
        window.rootViewController = rootViewController
        if let windowScene = window.windowScene
        {
            refreshUpdateLink = makeUIKitRefreshUpdateLink(for: windowScene)
        }
        if makeKey
        {
            window.makeKeyAndVisible()
        }
        else
        {
            window.isHidden = false
        }
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
    }

    func reset()
    {
        containerView.layer.removeAllAnimations()
        rootViewController.view.layer.removeAllAnimations()
        containerView.subviews.forEach
        {
            view in
            view.layer.removeAllAnimations()
            view.removeFromSuperview()
        }
        containerView.frame.origin = .zero
        containerView.frame.size = CGSize(width: 430, height: 932)
        containerView.setNeedsLayout()
        containerView.layoutIfNeeded()
        CATransaction.flush()
    }

    func prepareForMetalFrameCapture()
    {
        guard let window = installedWindow else
        {
            return
        }
        window.makeKeyAndVisible()
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        containerView.layoutIfNeeded()
        CATransaction.flush()
    }

    func mount(_ view: UIView, size: CGSize)
    {
        withPerfSignpost("screen.mount")
        {
            containerView.subviews.forEach { $0.removeFromSuperview() }
            view.frame = CGRect(origin: CGPoint(x: 24, y: 24), size: size)
            containerView.addSubview(view)
            commit(view)
        }
    }

    func commit(_ view: UIView)
    {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        withPerfSignpost("layout")
        {
            view.setNeedsLayout()
            view.layoutIfNeeded()
            view.layer.setNeedsLayout()
            view.layer.layoutIfNeeded()
        }
        withPerfSignpost("draw.encode")
        {
            if !(view.layer is CAMetalLayer)
            {
                view.layer.setNeedsDisplay()
                view.layer.displayIfNeeded()
            }
        }
        CATransaction.commit()
        withPerfSignpost("frame.present")
        {
            CATransaction.flush()
        }
    }

    private static func preferredWindowScene() -> UIWindowScene?
    {
        let scenes = UIApplication.shared.connectedScenes
            .compactMap
            {
                $0 as? UIWindowScene
            }
        return scenes.first
        {
            $0.activationState == .foregroundActive
        } ?? scenes.first
        {
            $0.activationState == .foregroundInactive
        } ?? scenes.first
    }
}

private enum OxideCameraRenderMode: Int32
{
    case nv12Optimized = 0
    case nv12Legacy = 1
    case bgraBenchmark = 2
}

private enum OxideCameraTextureSource: Int32
{
    case live = 0
    case syntheticBenchmark = 1
}

private enum OxideCameraVisiblePreviewTransport
{
    case oxideRenderer
    case avFoundationPreviewLayer
}

private final class LayeredCameraPreviewHostView: UIView
{
    private let metalView: UIView
    private let previewView: UIView

    init(metalView: UIView, previewView: UIView)
    {
        self.metalView = metalView
        self.previewView = previewView
        super.init(frame: .zero)
        backgroundColor = .white
        metalView.alpha = 0.001
        metalView.isUserInteractionEnabled = false
        addSubview(metalView)
        addSubview(previewView)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        metalView.frame = bounds
        previewView.frame = bounds
    }
}

@MainActor
private final class OxideCameraBenchmarkHarness
{
    private let host: PerfSurfaceHost
    private let hostView: UIView
    private let metalView: UIView
    private let layer: CAMetalLayer
    private let cameraSceneIndex: UInt32
    private let visibleTransport: OxideCameraVisiblePreviewTransport
    private let previewView: AVFoundationPreviewView?
    private let stageAccumulator = OxideCameraStageAccumulator()
    private let previewPlanAccumulator = OxideCameraPreviewPlanAccumulator()
    private let memoryAccumulator = OxideCameraMemoryAccumulator()
    private var recordStageMetrics = false
    private var currentMode: OxideCameraRenderMode = .nv12Optimized
    private var currentSource: OxideCameraTextureSource = .live
    private var contractSummaryCache: String?

    init?(
        host: PerfSurfaceHost,
        visibleTransport: OxideCameraVisiblePreviewTransport = .oxideRenderer
    )
    {
        guard let metalViewType = Self.resolveMetalViewType() else
        {
            recordBenchmarkBuildFailure("failed - camera preview benchmark could not resolve MetalView")
            return nil
        }
        let metalView = metalViewType.init(frame: .zero)
        guard let layer = metalView.layer as? CAMetalLayer else
        {
            recordBenchmarkBuildFailure("failed - camera preview benchmark MetalView did not expose CAMetalLayer")
            return nil
        }
        guard let sceneIndex = Self.resolveSceneIndex(named: "Camera") else
        {
            recordBenchmarkBuildFailure("failed - camera preview benchmark could not resolve Camera scene")
            return nil
        }
        self.host = host
        self.visibleTransport = visibleTransport
        self.metalView = metalView
        self.layer = layer
        self.cameraSceneIndex = sceneIndex
        if visibleTransport == .avFoundationPreviewLayer
        {
            let previewView = AVFoundationPreviewView(frame: .zero)
            previewView.previewLayer.videoGravity = .resizeAspectFill
            self.previewView = previewView
            self.hostView = LayeredCameraPreviewHostView(
                metalView: metalView,
                previewView: previewView
            )
        }
        else
        {
            self.previewView = nil
            self.hostView = metalView
        }
        configureDirectPreviewMetalLayer(view: metalView, layer: layer)
    }

    func installAndWarm(
        mode: OxideCameraRenderMode,
        source: OxideCameraTextureSource,
        warmupFrames: Int = 8
    ) -> Bool
    {
        currentMode = mode
        currentSource = source
        _ = oxideHostSetBenchmarkMode(1)
        host.mount(hostView, size: CGSize(width: 390, height: 844))
        host.prepareForMetalFrameCapture()
        guard initializeHost() else
        {
            recordBenchmarkBuildFailure("failed - camera preview benchmark host initialization returned non-zero")
            return false
        }
        _ = oxideHostSetCameraRenderMode(mode.rawValue)
        _ = oxideHostSetCameraTextureSource(source.rawValue)
        _ = oxideHostSetScene(cameraSceneIndex)
        _ = oxideHostSetCameraOptions(0, 0.0, 0, 0)
        let wantsLiveCamera = source == .live
        _ = oxideHostSetCameraRunningMode(wantsLiveCamera ? 1 : 0, wantsLiveCamera ? 1 : 0)
        let warmed: Bool
        if wantsLiveCamera
        {
            if visibleTransport == .avFoundationPreviewLayer
            {
                guard bindRunningOxideCameraSessionToPreviewLayer() else
                {
                    return false
                }
                warmed = warmHybridVisiblePreview(settleFrames: warmupFrames)
            }
            else
            {
                warmed = warmLiveCamera(settleFrames: warmupFrames)
            }
        }
        else
        {
            var ok = true
            for _ in 0..<warmupFrames
            {
                guard renderFrame(signpost: false) else
                {
                    ok = false
                    break
                }
            }
            warmed = ok
        }
        if warmed
        {
            refreshContractSummary()
        }
        return warmed
    }

    func renderFrame(signpost: Bool = true) -> Bool
    {
        if visibleTransport == .avFoundationPreviewLayer
        {
            return stepHybridVisiblePreview(signpost: signpost)
        }
        let tracePhases = signpost && cameraTracePhasesEnabled()
        let runFrame = { [self] () -> Bool in
            let (width, height, scale) = currentDrawableMetrics()
            let planT0 = perfNowMs()
            let (planResult, planReason): (Int32, Int32)
            if tracePhases
            {
                (planResult, planReason) = withPerfSignpost("camera.host.plan")
                {
                    (
                        oxideHostCameraPreviewPlan(width, height, scale),
                        oxideHostCameraPreviewPlanReason(width, height, scale)
                    )
                }
            }
            else
            {
                planResult = oxideHostCameraPreviewPlan(width, height, scale)
                planReason = oxideHostCameraPreviewPlanReason(width, height, scale)
            }
            let planMs = perfNowMs() - planT0
            if recordStageMetrics
            {
                previewPlanAccumulator.record(reason: planReason)
            }
            if planResult < 0
            {
                recordBenchmarkBuildFailure(
                    "failed - camera preview benchmark oxideHostCameraPreviewPlan returned \(planResult)"
                )
                return false
            }
            if planResult == 0
            {
                if recordStageMetrics
                {
                    stageAccumulator.recordSkippedFrame()
                    if let stats = readStats()
                    {
                        memoryAccumulator.record(
                            stats: stats,
                            drawableWidth: width,
                            drawableHeight: height,
                            layer: layer
                        )
                    }
                }
                return true
            }
            let drawableAcquireT0 = perfNowMs()
            let drawable: CAMetalDrawable?
            if tracePhases
            {
                drawable = withPerfSignpost("camera.drawable.acquire")
                {
                    layer.nextDrawable()
                }
            }
            else
            {
                drawable = layer.nextDrawable()
            }
            let drawableAcquireMs = perfNowMs() - drawableAcquireT0
            guard let drawable else
            {
                recordBenchmarkBuildFailure("failed - camera preview benchmark could not acquire CAMetalLayer drawable")
                return false
            }
            let drawablePtr = Unmanaged.passUnretained(drawable).toOpaque()
            let hostFrameT0 = perfNowMs()
            let frameResult: Int32
            if tracePhases
            {
                frameResult = withPerfSignpost("draw.encode")
                {
                    withPerfSignpost("camera.host.frame")
                    {
                        return oxideHostAppFrameWithDrawable(width, height, scale, drawablePtr)
                    }
                }
            }
            else
            {
                frameResult = oxideHostAppFrameWithDrawable(width, height, scale, drawablePtr)
            }
            let hostFrameMs = perfNowMs() - hostFrameT0
            if frameResult != 0
            {
                recordBenchmarkBuildFailure(
                    "failed - camera preview benchmark oxideHostAppFrameWithDrawable returned \(frameResult)"
                )
            }
            if frameResult == 0,
               recordStageMetrics,
               let stats = readStats()
            {
                stageAccumulator.record(
                    hostPlanMs: planMs,
                    drawableAcquireMs: drawableAcquireMs,
                    hostFrameMs: hostFrameMs,
                    hostTickTotalMs: planMs + drawableAcquireMs + hostFrameMs,
                    stats: stats
                )
                memoryAccumulator.record(
                    stats: stats,
                    drawableWidth: width,
                    drawableHeight: height,
                    layer: layer
                )
            }
            return frameResult == 0
        }
        if tracePhases
        {
            return withPerfSignpost("camera.host.tick_total", body: runFrame)
        }
        return runFrame()
    }

    func prepareForMeasuredPass(
        requiredFrameAdvances: Int = 2,
        timeout: TimeInterval = 1.5
    ) -> Bool
    {
        guard currentSource == .live else
        {
            return true
        }
        if visibleTransport == .avFoundationPreviewLayer
        {
            return waitForHybridVisiblePreviewAdvances(
                requiredFrameAdvances: requiredFrameAdvances,
                timeout: timeout,
                failurePrefix: "failed - hybrid visible preview did not advance after trace attach"
            )
        }
        let deadline = Date().addingTimeInterval(timeout)
        let pollInterval = min(
            resolveCameraBenchmarkOpportunityIntervalSeconds(
                maximumFramesPerSecond: host.containerView.window?.windowScene?.screen.maximumFramesPerSecond
            ),
            1.0 / 120.0
        )
        var observedAdvances = 0
        while Date() < deadline
        {
            let (width, height, scale) = currentDrawableMetrics()
            let planReason = oxideHostCameraPreviewPlanReason(width, height, scale)
            if planReason < 0
            {
                recordBenchmarkBuildFailure(
                    "failed - live camera preview preflight returned \(planReason) before measured pass"
                )
                return false
            }
            if (planReason & oxideCameraPreviewReasonMeasuredPassMask) != 0
            {
                guard renderFrame(signpost: false) else
                {
                    return false
                }
                if (planReason & (oxideCameraPreviewReasonNewTimestamp | oxideCameraPreviewReasonNewGeneration)) != 0
                {
                    observedAdvances += 1
                    if observedAdvances >= max(requiredFrameAdvances, 1)
                    {
                        return true
                    }
                }
            }
            RunLoop.main.run(until: Date().addingTimeInterval(pollInterval))
        }
        let stats = readStats()
        recordBenchmarkBuildFailure(
            "failed - live camera preview did not advance after trace attach " +
            "(advances=\(observedAdvances), running=\(stats?.camRunning ?? 0), " +
            "paused=\(stats?.camPaused ?? 0), size=\(stats?.camWidth ?? 0)x\(stats?.camHeight ?? 0))"
        )
        return false
    }

    func beginStageMeasurement()
    {
        stageAccumulator.reset()
        previewPlanAccumulator.reset()
        memoryAccumulator.reset()
        _ = oxideHostResetCameraPerfCounters()
        recordStageMetrics = true
    }

    func endStageMeasurement()
    {
        recordStageMetrics = false
    }

    func stageSummaryLine() -> String?
    {
        guard visibleTransport == .oxideRenderer else
        {
            return nil
        }
        return stageAccumulator.summaryLine()
    }

    func previewPlanSummaryLine() -> String?
    {
        guard visibleTransport == .oxideRenderer else
        {
            return nil
        }
        return previewPlanAccumulator.summaryLine()
    }

    func memorySummaryLine() -> String?
    {
        memoryAccumulator.summaryLine()
    }

    func contractSummaryLine() -> String?
    {
        contractSummaryCache
    }

    private func warmLiveCamera(
        settleFrames: Int,
        timeout: TimeInterval = 3.0
    ) -> Bool
    {
        let requiredFrames = max(settleFrames, 3)
        let deadline = Date().addingTimeInterval(timeout)
        var consecutiveReadyFrames = 0
        var probeCountdown = 0
        while Date() < deadline
        {
            guard renderFrame(signpost: false) else
            {
                return false
            }
            if probeCountdown > 0
            {
                probeCountdown -= 1
                RunLoop.main.run(until: Date().addingTimeInterval(1.0 / 120.0))
                continue
            }
            probeCountdown = 3
            guard let stats = readStats() else
            {
                recordBenchmarkBuildFailure("failed - live camera preview could not read host stats")
                return false
            }
            let hasFrame =
                stats.camRunning != 0 &&
                stats.camPaused == 0 &&
                stats.camWidth > 0 &&
                stats.camHeight > 0
            consecutiveReadyFrames = hasFrame ? (consecutiveReadyFrames + 1) : 0
            if consecutiveReadyFrames >= requiredFrames
            {
                return true
            }
            RunLoop.main.run(until: Date().addingTimeInterval(1.0 / 120.0))
        }
        let stats = readStats()
        recordBenchmarkBuildFailure(
            "failed - live camera preview did not produce frames " +
            "(running=\(stats?.camRunning ?? 0), paused=\(stats?.camPaused ?? 0), " +
            "size=\(stats?.camWidth ?? 0)x\(stats?.camHeight ?? 0))"
        )
        return false
    }

    private func bindRunningOxideCameraSessionToPreviewLayer() -> Bool
    {
        guard let previewView else
        {
            return false
        }
        guard let sessionPtr = oxideCamGetRunningSession() else
        {
            recordBenchmarkBuildFailure("failed - Oxide hybrid preview could not access the running camera session")
            return false
        }
        let session = Unmanaged<AVCaptureSession>
            .fromOpaque(sessionPtr)
            .takeUnretainedValue()
        previewView.previewLayer.session = session
        if let connection = previewView.previewLayer.connection
        {
            connection.automaticallyAdjustsVideoMirroring = false
            connection.isVideoMirrored = false
            let portraitAngle: CGFloat = 90.0
            if connection.isVideoRotationAngleSupported(portraitAngle)
            {
                connection.videoRotationAngle = portraitAngle
            }
        }
        return true
    }

    private func warmHybridVisiblePreview(
        settleFrames: Int,
        timeout: TimeInterval = 3.0
    ) -> Bool
    {
        waitForHybridVisiblePreviewAdvances(
            requiredFrameAdvances: max(settleFrames, 3),
            timeout: timeout,
            failurePrefix: "failed - hybrid visible preview did not produce advancing frames"
        )
    }

    private func waitForHybridVisiblePreviewAdvances(
        requiredFrameAdvances: Int,
        timeout: TimeInterval,
        failurePrefix: String
    ) -> Bool
    {
        let deadline = Date().addingTimeInterval(timeout)
        let pollInterval = min(
            resolveCameraBenchmarkOpportunityIntervalSeconds(
                maximumFramesPerSecond: host.containerView.window?.windowScene?.screen.maximumFramesPerSecond
            ),
            1.0 / 120.0
        )
        let requiredAdvances = max(requiredFrameAdvances, 1)
        var observedAdvances = 0
        var lastGeneration: UInt64 = 0
        var lastTimestampNs: UInt64 = 0
        var hasSeenFrameIdentity = false
        while Date() < deadline
        {
            guard stepHybridVisiblePreview(signpost: false) else
            {
                return false
            }
            guard let stats = readStats() else
            {
                recordBenchmarkBuildFailure("failed - hybrid visible preview could not read host stats")
                return false
            }
            let hasFrame =
                stats.camRunning != 0 &&
                stats.camPaused == 0 &&
                stats.camWidth > 0 &&
                stats.camHeight > 0
            if hasFrame
            {
                let currentGeneration = stats.camLatestPublishedGeneration
                let currentTimestampNs = stats.camLatestPublishedTimestampNs
                let hasFrameIdentity = currentGeneration > 0 || currentTimestampNs > 0
                if hasSeenFrameIdentity
                {
                    observedAdvances += oxideCameraFrameAdvanceCount(
                        previousGeneration: lastGeneration,
                        previousTimestampNs: lastTimestampNs,
                        currentGeneration: currentGeneration,
                        currentTimestampNs: currentTimestampNs
                    )
                }
                else if hasFrameIdentity
                {
                    hasSeenFrameIdentity = true
                }
                lastGeneration = currentGeneration
                lastTimestampNs = currentTimestampNs
                if observedAdvances >= requiredAdvances
                {
                    return true
                }
            }
            RunLoop.main.run(until: Date().addingTimeInterval(pollInterval))
        }
        let stats = readStats()
        recordBenchmarkBuildFailure(
            "\(failurePrefix) " +
            "(advances=\(observedAdvances), running=\(stats?.camRunning ?? 0), " +
            "paused=\(stats?.camPaused ?? 0), size=\(stats?.camWidth ?? 0)x\(stats?.camHeight ?? 0), " +
            "generation=\(stats?.camLatestPublishedGeneration ?? 0), " +
            "timestampNs=\(stats?.camLatestPublishedTimestampNs ?? 0))"
        )
        return false
    }

    private func stepHybridVisiblePreview(signpost: Bool) -> Bool
    {
        if signpost
        {
            withPerfSignpost("baseline.preview.step")
            {
                withPerfSignpost("baseline.preview.layout")
                {
                    host.containerView.layoutIfNeeded()
                }
                withPerfSignpost("baseline.preview.flush")
                {
                    CATransaction.flush()
                }
            }
        }
        else
        {
            host.containerView.layoutIfNeeded()
            CATransaction.flush()
        }
        if recordStageMetrics,
           let stats = readStats()
        {
            let (width, height, _) = currentDrawableMetrics()
            memoryAccumulator.record(
                stats: stats,
                drawableWidth: width,
                drawableHeight: height,
                layer: layer
            )
        }
        return true
    }

    func tearDown()
    {
        previewView?.previewLayer.session = nil
        _ = oxideHostSetCameraRunning(0)
        host.reset()
        CATransaction.flush()
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        _ = oxideHostSetBenchmarkMode(0)
    }

    private func readStats() -> OxideHostStats?
    {
        var stats = OxideHostStats()
        guard oxideHostAppStats(&stats) == 0 else
        {
            return nil
        }
        return stats
    }

    private func currentDrawableMetrics() -> (UInt32, UInt32, Float)
    {
        let drawableSize = layer.drawableSize
        let width = UInt32(max(Int(drawableSize.width.rounded()), 1))
        let height = UInt32(max(Int(drawableSize.height.rounded()), 1))
        let scale = Float(max(layer.contentsScale, 1.0))
        return (width, height, scale)
    }

    private static func resolveMetalViewType() -> UIView.Type?
    {
        if let metalViewType = NSClassFromString("MetalView") as? UIView.Type
        {
            return metalViewType
        }
        if let executable = Bundle.main.object(forInfoDictionaryKey: "CFBundleExecutable") as? String,
           let metalViewType = NSClassFromString("\(executable).MetalView") as? UIView.Type
        {
            return metalViewType
        }
        if let bundleName = Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String,
           let metalViewType = NSClassFromString("\(bundleName).MetalView") as? UIView.Type
        {
            return metalViewType
        }
        return nil
    }

    private func initializeHost() -> Bool
    {
        host.containerView.layoutIfNeeded()
        let drawableSize = layer.drawableSize
        let width = UInt32(max(Int(drawableSize.width.rounded()), 1))
        let height = UInt32(max(Int(drawableSize.height.rounded()), 1))
        let scale = Float(max(layer.contentsScale, 1.0))
        return oxideHostAppInit(width, height, scale) == 0
    }

    private func refreshContractSummary()
    {
        guard let stats = readStats() else
        {
            contractSummaryCache = nil
            return
        }
        let captureContractMode = resolveCameraCaptureContractMode()
        let requestedPixelFormat =
            currentMode == .bgraBenchmark ? "bgra8" : "420f"
        let activePixelFormat: String
        switch currentMode
        {
        case .bgraBenchmark:
            activePixelFormat = "bgra8"
        case .nv12Optimized, .nv12Legacy:
            activePixelFormat = stats.camVideoRange == 1 ? "420v" : "420f"
        }
        let payload = OxideCameraContractSummaryPayload(
            source: currentSource == .live
                ? (
                    visibleTransport == .avFoundationPreviewLayer
                    ? "oxide-live-hybrid"
                    : "oxide-live"
                )
                : "oxide-synthetic",
            transport: visibleTransport == .avFoundationPreviewLayer
                ? "AVCaptureVideoPreviewLayer+OxideCameraSidecar(NV12)"
                : (
                    currentMode == .bgraBenchmark
                    ? "AVCaptureVideoDataOutput+CVMetalTexture(BGRA)"
                    : "AVCaptureVideoDataOutput+CVMetalTexture(NV12)"
                ),
            devicePosition: "back",
            sessionPreset: captureContractMode.sessionPresetName,
            requestedPixelFormat: requestedPixelFormat,
            activePixelFormat: activePixelFormat,
            requestedWidth: benchmarkCameraTargetWidth,
            requestedHeight: benchmarkCameraTargetHeight,
            requestedFps: benchmarkCameraTargetFps,
            activeWidth: Int32(stats.camWidth),
            activeHeight: Int32(stats.camHeight),
            activeFps: Double(stats.camFps),
            videoRange: oxideCameraVideoRangeName(videoRange: stats.camVideoRange),
            colorSpace: oxideCameraColorSpaceName(colorSpace: stats.camColorSpace),
            wideColorAuto: false,
            mirrored: false
        )
        contractSummaryCache = encodeCameraContractSummaryLine(payload)
    }

    private static func resolveSceneIndex(named target: String) -> UInt32?
    {
        let count = Int(oxideHostSceneCount())
        for index in 0..<count
        {
            let needed = Int(oxideHostSceneName(UInt32(index), nil, 0))
            guard needed > 0 else
            {
                continue
            }
            let buffer = UnsafeMutablePointer<CChar>.allocate(capacity: needed)
            defer
            {
                buffer.deallocate()
            }
            guard oxideHostSceneName(UInt32(index), buffer, UInt32(needed)) > 0 else
            {
                continue
            }
            if String(cString: buffer) == target
            {
                return UInt32(index)
            }
        }
        return nil
    }
}

@MainActor
private final class OxideRealAppCameraBenchmarkHarness
{
    private let visibleTransport: OxideCameraVisiblePreviewTransport
    private let cameraSceneIndex: UInt32
    private let stageAccumulator = OxideCameraStageAccumulator()
    private let previewPlanAccumulator = OxideCameraPreviewPlanAccumulator()
    private let memoryAccumulator = OxideCameraMemoryAccumulator()
    private var currentMode: OxideCameraRenderMode = .nv12Optimized
    private var currentSource: OxideCameraTextureSource = .live
    private var contractSummaryCache: String?
    private var recordStageMetrics = false
    private var lastRecordedTickSerial: UInt64 = 0
    private var tickPolls = 0
    private var tickReadFailures = 0
    private var staleTickPolls = 0
    private var newSerials = 0
    private var statsReadFailures = 0
    private var recordedTicks = 0
    private var startSerial: UInt64 = 0
    private var lastObservedSerial: UInt64 = 0
    private var maxObservedSerial: UInt64 = 0
    private var skippedTicks = 0
    private var drawableAcquiredTicks = 0
    private var frameSubmittedTicks = 0

    init?(visibleTransport: OxideCameraVisiblePreviewTransport)
    {
        guard realAppCameraBenchmarkEnabled() else
        {
            recordBenchmarkBuildFailure(
                "failed - actual app camera benchmark requires \(perfCameraRealAppHostEnv)=1"
            )
            return nil
        }
        if visibleTransport == .avFoundationPreviewLayer &&
           !realAppCameraBenchmarkUsesHybridVisiblePreview()
        {
            recordBenchmarkBuildFailure(
                "failed - actual app hybrid camera benchmark requires " +
                "\(perfCameraRealAppHybridVisiblePreviewEnv)=1"
            )
            return nil
        }
        if visibleTransport == .oxideRenderer &&
           realAppCameraBenchmarkUsesHybridVisiblePreview()
        {
            recordBenchmarkBuildFailure(
                "failed - actual app custom camera benchmark must not enable " +
                "\(perfCameraRealAppHybridVisiblePreviewEnv)"
            )
            return nil
        }
        guard let sceneIndex = Self.resolveSceneIndex(named: "Camera") else
        {
            recordBenchmarkBuildFailure(
                "failed - actual app camera benchmark could not resolve Camera scene"
            )
            return nil
        }
        self.visibleTransport = visibleTransport
        self.cameraSceneIndex = sceneIndex
    }

    func installAndWarm(
        mode: OxideCameraRenderMode,
        source: OxideCameraTextureSource,
        warmupFrames: Int = 8
    ) -> Bool
    {
        guard source == .live else
        {
            recordBenchmarkBuildFailure(
                "failed - actual app camera benchmark only supports live camera source"
            )
            return false
        }
        currentMode = mode
        currentSource = source
        _ = oxideHostSetBenchmarkMode(1)
        _ = oxideHostSetCameraRenderMode(mode.rawValue)
        _ = oxideHostSetCameraTextureSource(source.rawValue)
        _ = oxideHostSetScene(cameraSceneIndex)
        _ = oxideHostSetCameraOptions(0, 0.0, 0, 0)
        _ = oxideHostSetCameraRunningMode(1, 1)
        return waitForFrameAdvances(
            requiredFrameAdvances: max(warmupFrames, 3),
            timeout: 3.0,
            failurePrefix: "failed - actual app live camera preview did not produce advancing frames"
        )
    }

    func prepareForMeasuredPass(
        requiredFrameAdvances: Int = 2,
        timeout: TimeInterval = 1.5
    ) -> Bool
    {
        waitForFrameAdvances(
            requiredFrameAdvances: requiredFrameAdvances,
            timeout: timeout,
            failurePrefix: visibleTransport == .avFoundationPreviewLayer
                ? "failed - actual app hybrid visible preview did not advance after trace attach"
                : "failed - actual app custom visible preview did not advance after trace attach"
        )
    }

    func step()
    {
        guard recordStageMetrics,
              visibleTransport == .oxideRenderer else
        {
            return
        }
        recordLatestMeasuredTick()
    }

    func beginStageMeasurement()
    {
        stageAccumulator.reset()
        previewPlanAccumulator.reset()
        memoryAccumulator.reset()
        let initialSerial = readTickPerf()?.serial ?? 0
        lastRecordedTickSerial = initialSerial
        startSerial = initialSerial
        lastObservedSerial = initialSerial
        maxObservedSerial = initialSerial
        tickPolls = 0
        tickReadFailures = 0
        staleTickPolls = 0
        newSerials = 0
        statsReadFailures = 0
        recordedTicks = 0
        skippedTicks = 0
        drawableAcquiredTicks = 0
        frameSubmittedTicks = 0
        _ = oxideHostResetCameraPerfCounters()
        recordStageMetrics = true
    }

    func endStageMeasurement()
    {
        recordStageMetrics = false
    }

    func stageSummaryLine() -> String?
    {
        guard visibleTransport == .oxideRenderer else
        {
            return nil
        }
        return stageAccumulator.summaryLine()
    }

    func previewPlanSummaryLine() -> String?
    {
        guard visibleTransport == .oxideRenderer else
        {
            return nil
        }
        return previewPlanAccumulator.summaryLine()
    }

    func tickDebugSummaryLine() -> String?
    {
        guard visibleTransport == .oxideRenderer else
        {
            return nil
        }
        return encodeTickDebugSummaryLine(
            startSerial: startSerial,
            lastObservedSerial: lastObservedSerial,
            maxObservedSerial: maxObservedSerial,
            polls: tickPolls,
            tickReadFailures: tickReadFailures,
            stalePolls: staleTickPolls,
            newSerials: newSerials,
            statsReadFailures: statsReadFailures,
            recordedTicks: recordedTicks,
            skippedTicks: skippedTicks,
            drawableAcquiredTicks: drawableAcquiredTicks,
            frameSubmittedTicks: frameSubmittedTicks
        )
    }

    func appHostDebugSummaryLine() -> String?
    {
        guard visibleTransport == .oxideRenderer,
              let debugPerf = readAppDebugPerf() else
        {
            return nil
        }
        return encodeAppHostDebugSummaryLine(
            OxideAppHostDebugSummaryPayload(
                sceneWillConnectCalls: debugPerf.sceneWillConnectCalls,
                perfSceneBranchCalls: debugPerf.perfSceneBranchCalls,
                normalSceneBranchCalls: debugPerf.normalSceneBranchCalls,
                metalViewInstalls: debugPerf.metalViewInstalls,
                displayLinkCreateCalls: debugPerf.displayLinkCreateCalls,
                sceneDidBecomeActiveCalls: debugPerf.sceneDidBecomeActiveCalls,
                sceneWillEnterForegroundCalls: debugPerf.sceneWillEnterForegroundCalls,
                ensureHostInitializedCalls: debugPerf.ensureHostInitializedCalls,
                hostReadyTransitions: debugPerf.hostReadyTransitions,
                onTickCalls: debugPerf.onTickCalls,
                runningUiTest: debugPerf.runningUiTest != 0,
                runningPerfBenchmarkHost: debugPerf.runningPerfBenchmarkHost != 0,
                shouldRender: debugPerf.shouldRender != 0,
                hostReady: debugPerf.hostReady != 0
            )
        )
    }

    func memorySummaryLine() -> String?
    {
        guard visibleTransport == .oxideRenderer else
        {
            return nil
        }
        return memoryAccumulator.summaryLine()
    }

    func contractSummaryLine() -> String?
    {
        contractSummaryCache
    }

    func tearDown()
    {
        _ = oxideHostSetCameraRunning(0)
        CATransaction.flush()
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        _ = oxideHostSetBenchmarkMode(0)
    }

    private func waitForFrameAdvances(
        requiredFrameAdvances: Int,
        timeout: TimeInterval,
        failurePrefix: String
    ) -> Bool
    {
        let deadline = Date().addingTimeInterval(timeout)
        let pollInterval = min(
            resolveCameraBenchmarkOpportunityIntervalSeconds(
                maximumFramesPerSecond: UIScreen.main.maximumFramesPerSecond
            ),
            1.0 / 120.0
        )
        let requiredAdvances = max(requiredFrameAdvances, 1)
        var observedAdvances = 0
        var lastGeneration: UInt64 = 0
        var lastTimestampNs: UInt64 = 0
        var hasSeenFrameIdentity = false
        while Date() < deadline
        {
            guard let stats = readStats() else
            {
                recordBenchmarkBuildFailure(
                    "failed - actual app camera benchmark could not read host stats"
                )
                return false
            }
            let hasFrame =
                stats.camRunning != 0 &&
                stats.camPaused == 0 &&
                stats.camWidth > 0 &&
                stats.camHeight > 0
            if hasFrame
            {
                let currentGeneration = stats.camLatestPublishedGeneration
                let currentTimestampNs = stats.camLatestPublishedTimestampNs
                let hasFrameIdentity = currentGeneration > 0 || currentTimestampNs > 0
                if hasSeenFrameIdentity
                {
                    observedAdvances += oxideCameraFrameAdvanceCount(
                        previousGeneration: lastGeneration,
                        previousTimestampNs: lastTimestampNs,
                        currentGeneration: currentGeneration,
                        currentTimestampNs: currentTimestampNs
                    )
                }
                else if hasFrameIdentity
                {
                    hasSeenFrameIdentity = true
                }
                lastGeneration = currentGeneration
                lastTimestampNs = currentTimestampNs
                if observedAdvances >= requiredAdvances
                {
                    refreshContractSummary(stats: stats)
                    return true
                }
            }
            RunLoop.main.run(until: Date().addingTimeInterval(pollInterval))
        }
        let stats = readStats()
        recordBenchmarkBuildFailure(
            "\(failurePrefix) " +
            "(advances=\(observedAdvances), running=\(stats?.camRunning ?? 0), " +
            "paused=\(stats?.camPaused ?? 0), size=\(stats?.camWidth ?? 0)x\(stats?.camHeight ?? 0), " +
            "generation=\(stats?.camLatestPublishedGeneration ?? 0), " +
            "timestampNs=\(stats?.camLatestPublishedTimestampNs ?? 0))"
        )
        return false
    }

    private func refreshContractSummary(stats: OxideHostStats)
    {
        let captureContractMode = resolveCameraCaptureContractMode()
        let requestedPixelFormat =
            currentMode == .bgraBenchmark ? "bgra8" : "420f"
        let activePixelFormat: String
        switch currentMode
        {
        case .bgraBenchmark:
            activePixelFormat = "bgra8"
        case .nv12Optimized, .nv12Legacy:
            activePixelFormat = stats.camVideoRange == 1 ? "420v" : "420f"
        }
        let payload = OxideCameraContractSummaryPayload(
            source: visibleTransport == .avFoundationPreviewLayer
                ? "oxide-live-app-hybrid"
                : "oxide-live-app-host",
            transport: visibleTransport == .avFoundationPreviewLayer
                ? "OxideAppHost+AVCaptureVideoPreviewLayer+OxideCameraSidecar(NV12)"
                : (
                    currentMode == .bgraBenchmark
                    ? "OxideAppHost+AVCaptureVideoDataOutput+CVMetalTexture(BGRA)"
                    : "OxideAppHost+AVCaptureVideoDataOutput+CVMetalTexture(NV12)"
                ),
            devicePosition: "back",
            sessionPreset: captureContractMode.sessionPresetName,
            requestedPixelFormat: requestedPixelFormat,
            activePixelFormat: activePixelFormat,
            requestedWidth: benchmarkCameraTargetWidth,
            requestedHeight: benchmarkCameraTargetHeight,
            requestedFps: benchmarkCameraTargetFps,
            activeWidth: Int32(stats.camWidth),
            activeHeight: Int32(stats.camHeight),
            activeFps: Double(stats.camFps),
            videoRange: oxideCameraVideoRangeName(videoRange: stats.camVideoRange),
            colorSpace: oxideCameraColorSpaceName(colorSpace: stats.camColorSpace),
            wideColorAuto: false,
            mirrored: false
        )
        contractSummaryCache = encodeCameraContractSummaryLine(payload)
    }

    private func readTickPerf() -> OxideHostCameraTickPerf?
    {
        var tickPerf = OxideHostCameraTickPerf()
        guard oxideHostCameraTickPerf(&tickPerf) == 0 else
        {
            return nil
        }
        return tickPerf
    }

    private func readStats() -> OxideHostStats?
    {
        var stats = OxideHostStats()
        guard oxideHostAppStats(&stats) == 0 else
        {
            return nil
        }
        return stats
    }

    private func readAppDebugPerf() -> OxideHostAppDebugPerf?
    {
        var debugPerf = OxideHostAppDebugPerf()
        guard oxideHostAppDebugPerf(&debugPerf) == 0 else
        {
            return nil
        }
        return debugPerf
    }

    private func recordLatestMeasuredTick()
    {
        tickPolls += 1
        guard let tickPerf = readTickPerf() else
        {
            tickReadFailures += 1
            return
        }
        lastObservedSerial = tickPerf.serial
        maxObservedSerial = max(maxObservedSerial, tickPerf.serial)
        guard tickPerf.serial > lastRecordedTickSerial else
        {
            staleTickPolls += 1
            return
        }
        newSerials += 1
        lastRecordedTickSerial = tickPerf.serial
        if tickPerf.skipped != 0
        {
            skippedTicks += 1
        }
        if tickPerf.drawableAcquired != 0
        {
            drawableAcquiredTicks += 1
        }
        if tickPerf.frameSubmitted != 0
        {
            frameSubmittedTicks += 1
        }
        let signedPlanReason = Int32(bitPattern: tickPerf.planReason)
        previewPlanAccumulator.record(reason: signedPlanReason)
        let stats = readStats()
        if stats == nil
        {
            statsReadFailures += 1
        }
        let stageStats = (tickPerf.skipped != 0 || stats == nil) ? OxideHostStats() : stats!
        stageAccumulator.record(
            hostPlanMs: Double(tickPerf.planMs),
            drawableAcquireMs: Double(tickPerf.drawableAcquireMs),
            hostFrameMs: Double(tickPerf.frameCallMs),
            hostTickTotalMs: Double(tickPerf.tickTotalMs),
            stats: stageStats
        )
        recordedTicks += 1
        if let stats
        {
            memoryAccumulator.record(
                stats: stats,
                drawableWidth: tickPerf.drawableWidth,
                drawableHeight: tickPerf.drawableHeight,
                pixelFormat: .bgra8Unorm,
                maximumDrawableCount: resolveDirectPreviewMaximumDrawableCount()
            )
        }
    }

    private static func resolveSceneIndex(named target: String) -> UInt32?
    {
        let count = Int(oxideHostSceneCount())
        for index in 0..<count
        {
            let needed = Int(oxideHostSceneName(UInt32(index), nil, 0))
            guard needed > 0 else
            {
                continue
            }
            let buffer = UnsafeMutablePointer<CChar>.allocate(capacity: needed)
            defer
            {
                buffer.deallocate()
            }
            guard oxideHostSceneName(UInt32(index), buffer, UInt32(needed)) > 0 else
            {
                continue
            }
            if String(cString: buffer) == target
            {
                return UInt32(index)
            }
        }
        return nil
    }
}

private final class AVFoundationPreviewView: UIView
{
    override class var layerClass: AnyClass
    {
        AVCaptureVideoPreviewLayer.self
    }

    var previewLayer: AVCaptureVideoPreviewLayer
    {
        guard let layer = self.layer as? AVCaptureVideoPreviewLayer else
        {
            fatalError("AVFoundationPreviewView expected AVCaptureVideoPreviewLayer")
        }
        return layer
    }
}

private final class AVFoundationPreviewDataOutputSink: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate
{
    private var deliveredFrames: UInt32 = 0

    func captureOutput(
        _ output: AVCaptureOutput,
        didOutput sampleBuffer: CMSampleBuffer,
        from connection: AVCaptureConnection
    )
    {
        _ = CMSampleBufferGetImageBuffer(sampleBuffer)
        _ = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        deliveredFrames &+= 1
    }

    func resetDeliveredFrames()
    {
        deliveredFrames = 0
    }

    func currentDeliveredFrames() -> UInt32
    {
        deliveredFrames
    }
}

@MainActor
private final class AVFoundationPreviewBenchmarkHarness
{
    private let host: PerfSurfaceHost
    private let previewView = AVFoundationPreviewView(frame: .zero)
    private let session = AVCaptureSession()
    private let sessionQueue = DispatchQueue(label: "com.oxide.perf.avfoundation.preview")
    private let videoQueue = DispatchQueue(label: "com.oxide.perf.avfoundation.preview.video")
    private let includeVideoDataOutputSidecar: Bool
    private var contractSummaryCache: String?
    private var videoOutput: AVCaptureVideoDataOutput?
    private var videoDataOutputSink: AVFoundationPreviewDataOutputSink?

    init?(host: PerfSurfaceHost, includeVideoDataOutputSidecar: Bool = false)
    {
        guard AVCaptureDevice.authorizationStatus(for: .video) == .authorized else
        {
            recordBenchmarkBuildFailure("failed - AVFoundation preview baseline requires camera permission")
            return nil
        }
        self.host = host
        self.includeVideoDataOutputSidecar = includeVideoDataOutputSidecar
    }

    func installAndWarm(warmupFrames: Int = 8) -> Bool
    {
        host.mount(previewView, size: CGSize(width: 390, height: 844))
        host.prepareForMetalFrameCapture()
        previewView.previewLayer.videoGravity = .resizeAspectFill
        guard configureSession() else
        {
            return false
        }
        previewView.previewLayer.session = session
        configurePreviewLayerConnection()
        refreshContractSummary()
        guard setSessionRunning(true) else
        {
            return false
        }
        for _ in 0..<max(warmupFrames, 8)
        {
            step(signpost: false)
        }
        return true
    }

    func step()
    {
        step(signpost: true)
    }

    func step(signpost: Bool)
    {
        if signpost
        {
            withPerfSignpost("baseline.preview.step")
            {
                withPerfSignpost("baseline.preview.layout")
                {
                    host.containerView.layoutIfNeeded()
                }
                withPerfSignpost("baseline.preview.flush")
                {
                    CATransaction.flush()
                }
            }
            return
        }
        host.containerView.layoutIfNeeded()
        CATransaction.flush()
    }

    func tearDown()
    {
        previewView.previewLayer.session = nil
        _ = setSessionRunning(false)
        host.reset()
        CATransaction.flush()
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
    }

    func contractSummaryLine() -> String?
    {
        contractSummaryCache
    }

    func prepareForMeasuredPass(settleFrames: Int = 2) -> Bool
    {
        videoDataOutputSink?.resetDeliveredFrames()
        let settleCount = max(settleFrames, 1)
        let settleInterval = 1.0 / Double(benchmarkCameraTargetFps)
        for _ in 0..<settleCount
        {
            step(signpost: false)
            RunLoop.main.run(until: Date().addingTimeInterval(settleInterval))
        }
        if includeVideoDataOutputSidecar && (videoDataOutputSink?.currentDeliveredFrames() ?? 0) == 0
        {
            recordBenchmarkBuildFailure(
                "failed - AVFoundation hybrid preview baseline did not deliver any video-data-output frames"
            )
            return false
        }
        return true
    }

    private func configureSession() -> Bool
    {
        var setupError: String?
        var configured = false
        let captureContractMode = resolveCameraCaptureContractMode()
        runOnSessionQueue
        {
            self.session.beginConfiguration()
            defer
            {
                self.session.commitConfiguration()
            }
            self.session.automaticallyConfiguresApplicationAudioSession = false
            if #available(iOS 10.0, *)
            {
                self.session.automaticallyConfiguresCaptureDeviceForWideColor = false
            }
            self.session.inputs.forEach
            {
                self.session.removeInput($0)
            }
            self.session.outputs.forEach
            {
                self.session.removeOutput($0)
            }
            guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back) else
            {
                setupError = "failed - AVFoundation preview baseline could not find the back camera"
                return
            }
            do
            {
                let input = try AVCaptureDeviceInput(device: device)
                guard self.session.canAddInput(input) else
                {
                    setupError = "failed - AVFoundation preview baseline could not add the camera input"
                    return
                }
                self.session.addInput(input)
            }
            catch
            {
                setupError = "failed - AVFoundation preview baseline input configuration threw \(error.localizedDescription)"
                return
            }
            do
            {
                try device.lockForConfiguration()
            }
            catch
            {
                setupError = "failed - AVFoundation preview baseline could not lock the camera device"
                return
            }
            switch captureContractMode
            {
            case .inputPriority:
                guard let format = preferredBenchmarkCameraFormat(for: device) else
                {
                    device.unlockForConfiguration()
                    setupError = "failed - AVFoundation preview baseline could not resolve a 720p-class NV12 format at 30 fps"
                    return
                }
                guard self.session.canSetSessionPreset(.inputPriority) else
                {
                    device.unlockForConfiguration()
                    setupError = "failed - AVFoundation preview baseline cannot set input-priority capture"
                    return
                }
                self.session.sessionPreset = .inputPriority
                device.activeFormat = format
                if #available(iOS 10.0, *)
                {
                    let supportedColorSpaces = format.supportedColorSpaces as NSArray
                    let supportsSRGB = supportedColorSpaces.contains(
                        NSNumber(value: AVCaptureColorSpace.sRGB.rawValue)
                    )
                    if supportsSRGB
                    {
                        device.activeColorSpace = .sRGB
                    }
                }
            case .preset720p:
                guard self.session.canSetSessionPreset(.hd1280x720) else
                {
                    device.unlockForConfiguration()
                    setupError = "failed - AVFoundation preview baseline cannot set 720p session preset"
                    return
                }
                self.session.sessionPreset = .hd1280x720
                if #available(iOS 10.0, *)
                {
                    let supportedColorSpaces = device.activeFormat.supportedColorSpaces as NSArray
                    let supportsSRGB = supportedColorSpaces.contains(
                        NSNumber(value: AVCaptureColorSpace.sRGB.rawValue)
                    )
                    if supportsSRGB
                    {
                        device.activeColorSpace = .sRGB
                    }
                }
            }
            let desired = CMTimeMake(value: 1, timescale: benchmarkCameraTargetFps)
            for range in device.activeFormat.videoSupportedFrameRateRanges
            {
                if CMTimeCompare(desired, range.minFrameDuration) >= 0 &&
                    CMTimeCompare(desired, range.maxFrameDuration) <= 0
                {
                    device.activeVideoMinFrameDuration = desired
                    device.activeVideoMaxFrameDuration = desired
                    break
                }
            }
            if self.includeVideoDataOutputSidecar
            {
                let output = AVCaptureVideoDataOutput()
                output.alwaysDiscardsLateVideoFrames = true
                output.videoSettings = [
                    kCVPixelBufferPixelFormatTypeKey as String:
                        Int(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange),
                ]
                let sink = AVFoundationPreviewDataOutputSink()
                output.setSampleBufferDelegate(sink, queue: self.videoQueue)
                guard self.session.canAddOutput(output) else
                {
                    device.unlockForConfiguration()
                    setupError =
                        "failed - AVFoundation hybrid preview baseline could not add AVCaptureVideoDataOutput"
                    return
                }
                self.session.addOutput(output)
                if let connection = output.connection(with: .video)
                {
                    connection.automaticallyAdjustsVideoMirroring = false
                    connection.isVideoMirrored = false
                    let portraitAngle: CGFloat = 90.0
                    if connection.isVideoRotationAngleSupported(portraitAngle)
                    {
                        connection.videoRotationAngle = portraitAngle
                    }
                }
                self.videoOutput = output
                self.videoDataOutputSink = sink
            }
            else
            {
                self.videoOutput = nil
                self.videoDataOutputSink = nil
            }
            device.unlockForConfiguration()
            configured = true
        }
        if let setupError
        {
            recordBenchmarkBuildFailure(setupError)
        }
        return configured
    }

    private func configurePreviewLayerConnection()
    {
        guard let connection = previewView.previewLayer.connection else
        {
            return
        }
        connection.automaticallyAdjustsVideoMirroring = false
        connection.isVideoMirrored = false
        let portraitAngle: CGFloat = 90.0
        if connection.isVideoRotationAngleSupported(portraitAngle)
        {
            connection.videoRotationAngle = portraitAngle
        }
    }

    private func refreshContractSummary()
    {
        guard let input = session.inputs.compactMap({ $0 as? AVCaptureDeviceInput }).first else
        {
            contractSummaryCache = nil
            return
        }
        let device = input.device
        let description = device.activeFormat.formatDescription
        let activePixelFormatCode = CMFormatDescriptionGetMediaSubType(description)
        let dimensions = CMVideoFormatDescriptionGetDimensions(description)
        let colorSpace: String
        if #available(iOS 10.0, *)
        {
            colorSpace = oxideCameraColorSpaceName(device.activeColorSpace)
        }
        else
        {
            colorSpace = "srgb"
        }
        let payload = OxideCameraContractSummaryPayload(
            source: includeVideoDataOutputSidecar
                ? "avfoundation-preview-layer-sidecar"
                : "avfoundation-preview-layer",
            transport: includeVideoDataOutputSidecar
                ? "AVCaptureVideoPreviewLayer+AVCaptureVideoDataOutput(NV12)"
                : "AVCaptureVideoPreviewLayer",
            devicePosition: "back",
            sessionPreset: resolveCameraCaptureContractMode().sessionPresetName,
            requestedPixelFormat: oxideCameraPixelFormatName(activePixelFormatCode),
            activePixelFormat: oxideCameraPixelFormatName(activePixelFormatCode),
            requestedWidth: benchmarkCameraTargetWidth,
            requestedHeight: benchmarkCameraTargetHeight,
            requestedFps: benchmarkCameraTargetFps,
            activeWidth: Int32(dimensions.width),
            activeHeight: Int32(dimensions.height),
            activeFps: oxideCameraFps(from: device.activeVideoMinFrameDuration),
            videoRange: oxideCameraVideoRangeName(pixelFormat: activePixelFormatCode),
            colorSpace: colorSpace,
            wideColorAuto: false,
            mirrored: false
        )
        contractSummaryCache = encodeCameraContractSummaryLine(payload)
    }

    private func setSessionRunning(_ running: Bool) -> Bool
    {
        var ok = true
        runOnSessionQueue
        {
            if running
            {
                if !self.session.isRunning
                {
                    self.session.startRunning()
                }
            }
            else if self.session.isRunning
            {
                self.session.stopRunning()
            }
        }
        if running && !session.isRunning
        {
            ok = false
            recordBenchmarkBuildFailure("failed - AVFoundation preview baseline did not enter the running state")
        }
        return ok
    }

    private func runOnSessionQueue(_ block: @escaping () -> Void)
    {
        let semaphore = DispatchSemaphore(value: 0)
        sessionQueue.async
        {
            block()
            semaphore.signal()
        }
        semaphore.wait()
    }
}

enum OxideUIKitRefreshMode: String
{
    case deviceDefault = "device-default"
    case capped60Hz = "60hz-capped"
    case native = "native"
}

func resolveUIKitRefreshMode(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> OxideUIKitRefreshMode
{
    switch environment[perfRefreshModeEnv]?.lowercased()
    {
    case "60", "60hz", "60hz-capped":
        return .capped60Hz
    case "native":
        return .native
    default:
        return .deviceDefault
    }
}

func resolveCameraBenchmarkTargetFramesPerSecond(
    maximumFramesPerSecond: Int? = UIScreen.main.maximumFramesPerSecond,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Int
{
    switch resolveUIKitRefreshMode(environment: environment)
    {
    case .capped60Hz:
        return 60
    case .deviceDefault, .native:
        return max(maximumFramesPerSecond ?? 60, 60)
    }
}

func resolveCameraBenchmarkOpportunityIntervalSeconds(
    maximumFramesPerSecond: Int? = UIScreen.main.maximumFramesPerSecond,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> TimeInterval
{
    1.0 / Double(
        resolveCameraBenchmarkTargetFramesPerSecond(
            maximumFramesPerSecond: maximumFramesPerSecond,
            environment: environment
        )
    )
}

func resolveCameraBenchmarkOpportunityCount(
    maximumFramesPerSecond: Int? = UIScreen.main.maximumFramesPerSecond,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> Int
{
    resolvePerfBenchmarkIterations(
        defaultValue: resolveCameraBenchmarkTargetFramesPerSecond(
            maximumFramesPerSecond: maximumFramesPerSecond,
            environment: environment
        ),
        minimum: Int(benchmarkCameraTargetFps),
        environment: environment
    )
}

func makeUIKitRefreshUpdateLink(
    for windowScene: UIWindowScene,
    environment: [String: String] = ProcessInfo.processInfo.environment
 ) -> UIUpdateLink?
{
    guard #available(iOS 18.0, *) else
    {
        return nil
    }
    switch resolveUIKitRefreshMode(environment: environment)
    {
    case .deviceDefault:
        return nil
    case .capped60Hz:
        let value: Float = 60.0
        let updateLink = UIUpdateLink(windowScene: windowScene)
        updateLink.preferredFrameRateRange = CAFrameRateRange(
            minimum: value,
            maximum: value,
            preferred: value
        )
        updateLink.isEnabled = true
        return updateLink
    case .native:
        let maximum = Float(max(windowScene.screen.maximumFramesPerSecond, 60))
        let updateLink = UIUpdateLink(windowScene: windowScene)
        updateLink.preferredFrameRateRange = CAFrameRateRange(
            minimum: min(60.0, maximum),
            maximum: maximum,
            preferred: maximum
        )
        updateLink.isEnabled = true
        return updateLink
    }
}

@MainActor
struct OxideUIKitBenchmark
{
    let testName: String
    let iterations: Int
    let signpostNames: [String]
    let prepareIteration: () -> Bool
    let summaryLines: () -> [String]
    let tearDown: () -> Void
    let runStep: () -> Void

    init(
        testName: String,
        iterations: Int,
        signpostNames: [String] = [],
        prepareIteration: @escaping () -> Bool = { true },
        summaryLines: @escaping () -> [String] = { [] },
        tearDown: @escaping () -> Void = {},
        runStep: @escaping () -> Void
    )
    {
        self.testName = testName
        self.iterations = iterations
        self.signpostNames = signpostNames
        self.prepareIteration = prepareIteration
        self.summaryLines = summaryLines
        self.tearDown = tearDown
        self.runStep = runStep
    }
}

@MainActor
private func uniqueBenchmarkSignpostNames(_ names: [String]) -> [String]
{
    var seen = Set<String>()
    return names.filter
    {
        seen.insert($0).inserted
    }
}

@MainActor
private func oxideCameraBenchmarkSignpostNames(
    mode: OxideCameraRenderMode,
    source: OxideCameraTextureSource,
    visibleTransport: OxideCameraVisiblePreviewTransport
) -> [String]
{
    if visibleTransport == .avFoundationPreviewLayer
    {
        return avFoundationPreviewBenchmarkSignpostNames
    }
    var names = [
        "camera.drawable.acquire",
        "camera.host.frame",
        "camera.renderer.resize",
        "camera.renderer.direct_preview",
        "camera.renderer.direct.setup",
        "camera.renderer.direct.encode_quad",
        "camera.renderer.direct.present_drawable",
        "camera.renderer.direct.commit",
        "draw.encode",
    ]
    if source == .live
    {
        switch mode
        {
        case .bgraBenchmark:
            names.append("camera.fetch.live_bgra")
        case .nv12Legacy, .nv12Optimized:
            names += [
                "camera.capture.total",
                "camera.capture.sample_setup",
                "camera.capture.lock",
                "camera.capture.texture_bridge",
                "camera.capture.publish",
                "camera.capture.publish.lock",
                "camera.capture.publish.texture_refs",
                "camera.capture.publish.pixel_buffer",
                "camera.capture.frame_delivery",
            ]
            names.append("camera.fetch.live_yuv")
        }
    }
    return uniqueBenchmarkSignpostNames(names)
}

@MainActor
private let avFoundationPreviewBenchmarkSignpostNames = [
    "baseline.preview.step",
    "baseline.preview.layout",
    "baseline.preview.flush",
    "baseline.preview.runloop",
]

@MainActor
func runPacedCameraPreviewWindow(
    opportunities: Int,
    opportunityIntervalSeconds: TimeInterval,
    waitSignpostName: StaticString? = nil,
    step: () -> Void
)
{
    guard opportunities > 0 else
    {
        return
    }
    let monotonicStart = CACurrentMediaTime()
    for stepIndex in 0..<opportunities
    {
        step()
        let targetTime = monotonicStart + (Double(stepIndex + 1) * opportunityIntervalSeconds)
        let now = CACurrentMediaTime()
        let sleepSeconds = max(targetTime - now, 0)
        if sleepSeconds > 0
        {
            if let waitSignpostName
            {
                withPerfSignpost(waitSignpostName)
                {
                    RunLoop.main.run(until: Date(timeIntervalSinceNow: sleepSeconds))
                }
            }
            else
            {
                RunLoop.main.run(until: Date(timeIntervalSinceNow: sleepSeconds))
            }
        }
    }
}

@MainActor
func runMeasuredBenchmarkPass(_ benchmark: OxideUIKitBenchmark)
{
    guard benchmark.prepareIteration() else
    {
        return
    }
    autoreleasepool
    {
        let signpostID = OSSignpostID(log: perfSignpostLog)
        os_signpost(.begin, log: perfSignpostLog, name: "PerfWorkload", signpostID: signpostID)
        for _ in 0..<benchmark.iterations
        {
            benchmark.runStep()
        }
        os_signpost(.end, log: perfSignpostLog, name: "PerfWorkload", signpostID: signpostID)
    }
}

enum OxideUIKitLaunchScenario: String
{
    case simpleHome = "simple_home"
    case heavyHome = "heavy_home"
    case detailRoute = "detail_route"
}

func resolveUIKitLaunchScenario(
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> (scenario: OxideUIKitLaunchScenario, route: String?)?
{
    let arguments = ProcessInfo.processInfo.arguments
    let hasLaunchEnv = environment[perfUIKitLaunchEnv].map({ !$0.isEmpty && $0 != "0" }) == true
    let hasLaunchArg = arguments.contains(perfUIKitLaunchArg)
    guard hasLaunchEnv || hasLaunchArg else
    {
        return nil
    }
    let route = environment[perfLaunchRouteEnv] ?? value(forLaunchArgument: perfLaunchRouteArg, arguments: arguments)
    if let rawScenario = environment[perfLaunchScenarioEnv] ?? value(forLaunchArgument: perfLaunchScenarioArg, arguments: arguments),
       let scenario = OxideUIKitLaunchScenario(rawValue: rawScenario)
    {
        return (scenario, route)
    }
    if route != nil
    {
        return (.detailRoute, route)
    }
    return (.simpleHome, nil)
}

private func value(forLaunchArgument name: String, arguments: [String]) -> String?
{
    guard let index = arguments.firstIndex(of: name) else
    {
        return nil
    }
    let valueIndex = arguments.index(after: index)
    guard valueIndex < arguments.endIndex else
    {
        return nil
    }
    return arguments[valueIndex]
}

@MainActor
func makeUIKitLaunchRootViewController(
    scenario: OxideUIKitLaunchScenario,
    route: String?
) -> UIViewController
{
    let controller = UIViewController()
    controller.view = UIView(frame: UIScreen.main.bounds)
    controller.view.backgroundColor = UIColor(red: 0.95, green: 0.97, blue: 1.0, alpha: 1.0)
    controller.view.isAccessibilityElement = true
    controller.view.accessibilityIdentifier = "uikitLaunchRoot"

    let readyLabel = UILabel(frame: .zero)
    readyLabel.font = .systemFont(ofSize: 14.0, weight: .semibold)
    readyLabel.textColor = UIColor(red: 0.14, green: 0.18, blue: 0.24, alpha: 1.0)
    readyLabel.isAccessibilityElement = true
    readyLabel.accessibilityIdentifier = "launchReadyLabel"
    readyLabel.numberOfLines = 2
    controller.view.addSubview(readyLabel)

    let contentView: UIView
    let contentFrame: CGRect

    switch scenario
    {
    case .simpleHome:
        let view = ControlSetBenchView(
            frame: .zero,
            image: OxideUIKitBenchmarkAssets.shared.checkerImage
        )
        view.installDeck(palettePhase: 0)
        readyLabel.text = "UIKit simple home ready"
        contentView = view
        contentFrame = CGRect(x: 18.0, y: 96.0, width: 390.0, height: 228.0)
    case .heavyHome:
        let view = CollectionBenchView(frame: .zero, mode: .feed)
        view.scroll(to: 0.18)
        readyLabel.text = "UIKit heavy home ready"
        contentView = view
        contentFrame = CGRect(x: 0.0, y: 88.0, width: controller.view.bounds.width, height: controller.view.bounds.height - 88.0)
    case .detailRoute:
        let view = LaunchDetailBenchView(
            frame: .zero,
            image: OxideUIKitBenchmarkAssets.shared.checkerImage
        )
        view.install(route: route ?? "oxide://detail/integration")
        readyLabel.text = "UIKit detail route ready"
        contentView = view
        contentFrame = CGRect(x: 18.0, y: 96.0, width: 390.0, height: 420.0)
    }

    readyLabel.frame = CGRect(x: 18.0, y: 44.0, width: 390.0, height: 36.0)
    contentView.frame = contentFrame
    contentView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    controller.view.addSubview(contentView)
    controller.view.layoutIfNeeded()
    return controller
}

private final class LaunchDetailBenchView: UIView
{
    private let heroImageView: UIImageView
    private let titleLabel = UILabel()
    private let subtitleLabel = UILabel()
    private let routeLabel = UILabel()
    private let bodyLabel = UILabel()

    init(frame: CGRect, image: UIImage)
    {
        self.heroImageView = UIImageView(image: image)
        super.init(frame: frame)
        backgroundColor = UIColor.white
        layer.cornerRadius = 20.0
        layer.shadowColor = UIColor.black.cgColor
        layer.shadowOpacity = 0.10
        layer.shadowRadius = 18.0
        layer.shadowOffset = CGSize(width: 0.0, height: 10.0)

        heroImageView.clipsToBounds = true
        heroImageView.contentMode = .scaleAspectFill
        heroImageView.layer.cornerRadius = 18.0

        titleLabel.font = .systemFont(ofSize: 24.0, weight: .bold)
        titleLabel.textColor = UIColor(red: 0.10, green: 0.12, blue: 0.18, alpha: 1.0)

        subtitleLabel.font = .systemFont(ofSize: 14.0, weight: .semibold)
        subtitleLabel.textColor = UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0)

        routeLabel.font = .monospacedSystemFont(ofSize: 12.0, weight: .regular)
        routeLabel.textColor = UIColor(red: 0.36, green: 0.40, blue: 0.48, alpha: 1.0)
        routeLabel.numberOfLines = 2

        bodyLabel.font = .systemFont(ofSize: 14.0, weight: .regular)
        bodyLabel.textColor = UIColor(red: 0.20, green: 0.24, blue: 0.30, alpha: 1.0)
        bodyLabel.numberOfLines = 0

        addSubview(heroImageView)
        addSubview(titleLabel)
        addSubview(subtitleLabel)
        addSubview(routeLabel)
        addSubview(bodyLabel)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        heroImageView.frame = CGRect(x: 18.0, y: 18.0, width: bounds.width - 36.0, height: 172.0)
        titleLabel.frame = CGRect(x: 20.0, y: 206.0, width: bounds.width - 40.0, height: 30.0)
        subtitleLabel.frame = CGRect(x: 20.0, y: 242.0, width: bounds.width - 40.0, height: 20.0)
        routeLabel.frame = CGRect(x: 20.0, y: 272.0, width: bounds.width - 40.0, height: 34.0)
        bodyLabel.frame = CGRect(x: 20.0, y: 316.0, width: bounds.width - 40.0, height: bounds.height - 336.0)
    }

    func install(route: String)
    {
        titleLabel.text = "Integration Detail"
        subtitleLabel.text = "Deep-link parity route"
        routeLabel.text = route
        bodyLabel.text = "Detail launch shows the selected payload with the same image bytes, rounded card treatment, and text stack used by the other parity screens."
    }
}

private final class UInt64Box
{
    var value: UInt64 = 0
}

private enum BenchPermissionStatus: UInt64
{
    case notDetermined = 0
    case denied = 1
    case limited = 2
    case authorized = 3
}

private struct BenchPermissionState
{
    let status: BenchPermissionStatus
    let timestampMs: UInt64
}

private final class PermissionBenchBridge
{
    typealias Callback = (BenchPermissionState) -> Void

    private var states: [String: BenchPermissionState]
    private var listeners: [String: [Int: Callback]] = [:]
    private var nextID = 1

    init(domain: String, status: BenchPermissionStatus)
    {
        self.states = [domain: BenchPermissionState(status: status, timestampMs: 0)]
    }

    func status(for domain: String) -> BenchPermissionStatus
    {
        states[domain]?.status ?? .notDetermined
    }

    @discardableResult
    func subscribe(domain: String, callback: @escaping Callback) -> Int
    {
        let id = nextID
        nextID += 1
        listeners[domain, default: [:]][id] = callback
        if let state = states[domain]
        {
            callback(state)
        }
        return id
    }

    func update(domain: String, status: BenchPermissionStatus, timestampMs: UInt64)
    {
        let state = BenchPermissionState(status: status, timestampMs: timestampMs)
        states[domain] = state
        listeners[domain]?.values.forEach
        {
            callback in
            callback(state)
        }
    }
}

private struct BenchLocationSample
{
    let latitudeDeg: Double
    let longitudeDeg: Double
    let timestampMs: UInt64
}

private struct BenchLocationSnapshot
{
    let last: BenchLocationSample?
    let historyCount: Int
}

private final class SensorLocationBenchBridge
{
    private let historyLimit: Int
    private var authorized = false
    private var last: BenchLocationSample?
    private var history: [BenchLocationSample] = []

    init(historyLimit: Int)
    {
        self.historyLimit = historyLimit
    }

    func updatePermission(authorized: Bool)
    {
        self.authorized = authorized
        guard !authorized else
        {
            return
        }
        last = nil
        history.removeAll(keepingCapacity: true)
    }

    func handleLocation(_ sample: BenchLocationSample)
    {
        guard authorized else
        {
            return
        }
        last = sample
        history.append(sample)
        if history.count > historyLimit
        {
            history.removeFirst(history.count - historyLimit)
        }
    }

    func snapshot() -> BenchLocationSnapshot
    {
        BenchLocationSnapshot(last: last, historyCount: history.count)
    }
}

private struct BenchBluetoothDevice
{
    let id: UInt64
    let lastSeenMs: UInt64
    let rssiDbm: Int
}

private struct BenchBluetoothSnapshot
{
    let poweredOn: Bool
    let deviceCount: Int
}

private final class BluetoothBenchBridge
{
    private let cacheLimit: Int
    private var authorized = false
    private var poweredOn = false
    private var devices: [UInt64: BenchBluetoothDevice] = [:]

    init(cacheLimit: Int)
    {
        self.cacheLimit = cacheLimit
    }

    func updatePermission(authorized: Bool)
    {
        self.authorized = authorized
        guard authorized else
        {
            poweredOn = false
            devices.removeAll(keepingCapacity: true)
            return
        }
    }

    func handleStateChanged(poweredOn: Bool)
    {
        guard authorized || !poweredOn else
        {
            return
        }
        self.poweredOn = poweredOn
        if !poweredOn
        {
            devices.removeAll(keepingCapacity: true)
        }
    }

    func handleDiscovery(_ device: BenchBluetoothDevice)
    {
        guard authorized && poweredOn else
        {
            return
        }
        devices[device.id] = device
        if devices.count > cacheLimit,
           let oldest = devices.values.min(by: { $0.lastSeenMs < $1.lastSeenMs })
        {
            devices.removeValue(forKey: oldest.id)
        }
    }

    func snapshot() -> BenchBluetoothSnapshot
    {
        BenchBluetoothSnapshot(poweredOn: poweredOn, deviceCount: devices.count)
    }
}

private final class ProgressBarBenchView: UIView
{
    private let trackLayer = CALayer()
    private let fillLayer = CALayer()

    var progress: CGFloat? = 0.6
    var phase: CGFloat = 0.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        layer.addSublayer(trackLayer)
        layer.addSublayer(fillLayer)
        trackLayer.backgroundColor = UIColor(white: 0.85, alpha: 1.0).cgColor
        fillLayer.backgroundColor = UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0).cgColor
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        trackLayer.frame = bounds
        trackLayer.cornerRadius = 4.0
        fillLayer.cornerRadius = 4.0
        if let progress
        {
            fillLayer.frame = CGRect(x: 0, y: 0, width: bounds.width * progress, height: bounds.height)
        }
        else
        {
            let width = max(bounds.width * 0.3, 8.0)
            let x = (bounds.width - width) * phase
            fillLayer.frame = CGRect(x: x, y: 0, width: width, height: bounds.height)
        }
    }
}

private final class SpinnerBenchView: UIView
{
    private let ringLayer = CAShapeLayer()
    var phase: CGFloat = 0.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        ringLayer.fillColor = UIColor.clear.cgColor
        ringLayer.strokeColor = UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0).cgColor
        ringLayer.lineCap = .round
        layer.addSublayer(ringLayer)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        let inset: CGFloat = 3.0
        ringLayer.frame = bounds
        ringLayer.lineWidth = 2.5
        ringLayer.path = UIBezierPath(
            ovalIn: bounds.insetBy(dx: inset, dy: inset)
        ).cgPath
        ringLayer.strokeStart = phase
        ringLayer.strokeEnd = min(phase + 0.35, 1.0)
        ringLayer.transform = CATransform3DMakeRotation(phase * .pi * 2.0, 0, 0, 1)
    }
}

private final class ToggleBenchView: UIView
{
    private let trackLayer = CALayer()
    private let thumbLayer = CALayer()
    var phase: CGFloat = 0.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        layer.addSublayer(trackLayer)
        layer.addSublayer(thumbLayer)
        thumbLayer.backgroundColor = UIColor.white.cgColor
        thumbLayer.shadowColor = UIColor.black.withAlphaComponent(0.12).cgColor
        thumbLayer.shadowOpacity = 1.0
        thumbLayer.shadowRadius = 1.0
        thumbLayer.shadowOffset = CGSize(width: 0, height: 1)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        let clamped = min(max(phase, 0.0), 1.0)
        let trackColor = UIColor(
            red: 0.80 - 0.40 * clamped,
            green: 0.82 + 0.06 * clamped,
            blue: 0.86 + 0.02 * clamped,
            alpha: 1.0
        )
        trackLayer.frame = bounds
        trackLayer.cornerRadius = bounds.height * 0.5
        trackLayer.backgroundColor = trackColor.cgColor

        let thumbDiameter = max(bounds.height - 6.0, 2.0)
        let x0 = 3.0
        let x1 = bounds.width - thumbDiameter - 3.0
        let x = x0 + (x1 - x0) * clamped
        thumbLayer.frame = CGRect(x: x, y: 3.0, width: thumbDiameter, height: thumbDiameter)
        thumbLayer.cornerRadius = thumbDiameter * 0.5
    }
}

private final class TimelineBenchView: UIView
{
    private var barLayers: [CALayer] = (0..<12).map { _ in CALayer() }
    var phase: CGFloat = 0.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        barLayers.forEach { layer.addSublayer($0) }
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        let spacing: CGFloat = 8.0
        let barWidth = (bounds.width - spacing * CGFloat(barLayers.count - 1)) / CGFloat(barLayers.count)
        let maxHeight = bounds.height
        for (index, bar) in barLayers.enumerated()
        {
            let offset = phase * .pi * 2.0 + CGFloat(index) * 0.35
            let normalized = (sin(offset) * 0.5) + 0.5
            let height = max(12.0, maxHeight * normalized)
            let x = CGFloat(index) * (barWidth + spacing)
            bar.frame = CGRect(x: x, y: maxHeight - height, width: barWidth, height: height)
            bar.backgroundColor = UIColor(
                red: 0.20 + 0.02 * CGFloat(index),
                green: 0.55,
                blue: 1.0 - 0.03 * CGFloat(index),
                alpha: 1.0
            ).cgColor
            bar.cornerRadius = min(6.0, barWidth * 0.5)
        }
    }
}

private final class OptimizedProgressBarBenchView: UIView
{
    var progress: CGFloat? = 0.6
    {
        didSet
        {
            setNeedsDisplay()
        }
    }
    var phase: CGFloat = 0.0
    {
        didSet
        {
            setNeedsDisplay()
        }
    }

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect)
    {
        guard let context = UIGraphicsGetCurrentContext() else
        {
            return
        }
        let trackPath = UIBezierPath(roundedRect: bounds, cornerRadius: 4.0)
        context.setFillColor(UIColor(white: 0.85, alpha: 1.0).cgColor)
        context.addPath(trackPath.cgPath)
        context.fillPath()

        let fillRect: CGRect
        if let progress
        {
            fillRect = CGRect(x: 0.0, y: 0.0, width: bounds.width * progress, height: bounds.height)
        }
        else
        {
            let width = max(bounds.width * 0.3, 8.0)
            let x = (bounds.width - width) * phase
            fillRect = CGRect(x: x, y: 0.0, width: width, height: bounds.height)
        }
        let fillPath = UIBezierPath(roundedRect: fillRect, cornerRadius: 4.0)
        context.setFillColor(UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0).cgColor)
        context.addPath(fillPath.cgPath)
        context.fillPath()
    }
}

private final class OptimizedSpinnerBenchView: UIView
{
    var phase: CGFloat = 0.0
    {
        didSet
        {
            setNeedsDisplay()
        }
    }

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect)
    {
        guard let context = UIGraphicsGetCurrentContext() else
        {
            return
        }
        let inset: CGFloat = 3.0
        let ringRect = bounds.insetBy(dx: inset, dy: inset)
        let radius = max(min(ringRect.width, ringRect.height) * 0.5, 1.0)
        let center = CGPoint(x: ringRect.midX, y: ringRect.midY)
        let startAngle = (phase * .pi * 2.0) - (.pi * 0.5)
        let endAngle = startAngle + (.pi * 0.7)

        context.setStrokeColor(UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0).cgColor)
        context.setLineWidth(2.5)
        context.setLineCap(.round)
        context.addArc(
            center: center,
            radius: radius,
            startAngle: startAngle,
            endAngle: endAngle,
            clockwise: false
        )
        context.strokePath()
    }
}

private final class OptimizedToggleBenchView: UIView
{
    var phase: CGFloat = 0.0
    {
        didSet
        {
            setNeedsDisplay()
        }
    }

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect)
    {
        guard let context = UIGraphicsGetCurrentContext() else
        {
            return
        }
        let clamped = min(max(phase, 0.0), 1.0)
        let trackColor = UIColor(
            red: 0.80 - 0.40 * clamped,
            green: 0.82 + 0.06 * clamped,
            blue: 0.86 + 0.02 * clamped,
            alpha: 1.0
        )
        let trackPath = UIBezierPath(roundedRect: bounds, cornerRadius: bounds.height * 0.5)
        context.setFillColor(trackColor.cgColor)
        context.addPath(trackPath.cgPath)
        context.fillPath()

        let thumbDiameter = max(bounds.height - 6.0, 2.0)
        let x0 = CGFloat(3.0)
        let x1 = bounds.width - thumbDiameter - 3.0
        let x = x0 + (x1 - x0) * clamped
        let thumbRect = CGRect(x: x, y: 3.0, width: thumbDiameter, height: thumbDiameter)
        let thumbPath = UIBezierPath(ovalIn: thumbRect)

        context.saveGState()
        context.setShadow(
            offset: CGSize(width: 0.0, height: 1.0),
            blur: 1.0,
            color: UIColor.black.withAlphaComponent(0.12).cgColor
        )
        context.setFillColor(UIColor.white.cgColor)
        context.addPath(thumbPath.cgPath)
        context.fillPath()
        context.restoreGState()
    }
}

private final class OptimizedButtonBenchView: UIView
{
    var scale: CGFloat = 1.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect)
    {
        let clampedScale = min(max(scale, 0.92), 1.0)
        let insetX = bounds.width * (1.0 - clampedScale) * 0.5
        let insetY = bounds.height * (1.0 - clampedScale) * 0.5
        let buttonRect = bounds.insetBy(dx: insetX, dy: insetY)
        let path = UIBezierPath(roundedRect: buttonRect, cornerRadius: 10.0)
        UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0).setFill()
        path.fill()

        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.alignment = .center
        let attributes: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 16.0, weight: .semibold),
            .foregroundColor: UIColor.white,
            .paragraphStyle: paragraphStyle,
        ]
        let titleRect = CGRect(
            x: buttonRect.minX,
            y: buttonRect.midY - 10.0,
            width: buttonRect.width,
            height: 20.0
        )
        ("Tap" as NSString).draw(in: titleRect, withAttributes: attributes)
    }
}

private final class OptimizedSliderBenchView: UIView
{
    var value: CGFloat = 0.0
    {
        didSet
        {
            setNeedsDisplay()
        }
    }

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect)
    {
        guard let context = UIGraphicsGetCurrentContext() else
        {
            return
        }
        let clamped = min(max(value, 0.0), 1.0)
        let trackRect = CGRect(
            x: 12.0,
            y: bounds.midY - 2.0,
            width: max(bounds.width - 24.0, 8.0),
            height: 4.0
        )
        let fillRect = CGRect(x: trackRect.minX, y: trackRect.minY, width: trackRect.width * clamped, height: trackRect.height)
        let thumbCenterX = trackRect.minX + trackRect.width * clamped
        let thumbRect = CGRect(x: thumbCenterX - 10.0, y: bounds.midY - 10.0, width: 20.0, height: 20.0)

        context.setFillColor(UIColor(white: 0.84, alpha: 1.0).cgColor)
        context.addPath(UIBezierPath(roundedRect: trackRect, cornerRadius: 2.0).cgPath)
        context.fillPath()

        context.setFillColor(UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0).cgColor)
        context.addPath(UIBezierPath(roundedRect: fillRect, cornerRadius: 2.0).cgPath)
        context.fillPath()

        context.saveGState()
        context.setShadow(
            offset: CGSize(width: 0.0, height: 1.0),
            blur: 2.0,
            color: UIColor.black.withAlphaComponent(0.14).cgColor
        )
        context.setFillColor(UIColor.white.cgColor)
        context.addPath(UIBezierPath(ovalIn: thumbRect).cgPath)
        context.fillPath()
        context.restoreGState()
    }
}

private final class OptimizedImageTransformBenchView: UIView
{
    private let image: UIImage
    var scale: CGFloat = 1.0
    {
        didSet
        {
            setNeedsDisplay()
        }
    }
    var offset = CGPoint.zero
    {
        didSet
        {
            setNeedsDisplay()
        }
    }

    init(frame: CGRect, image: UIImage)
    {
        self.image = image
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect)
    {
        let clipPath = UIBezierPath(roundedRect: bounds, cornerRadius: 12.0)
        clipPath.addClip()
        let imageSize = image.size
        guard imageSize.width > 0.0, imageSize.height > 0.0 else
        {
            return
        }
        let fitScale = min(bounds.width / imageSize.width, bounds.height / imageSize.height)
        let drawSize = CGSize(
            width: imageSize.width * fitScale * scale,
            height: imageSize.height * fitScale * scale
        )
        let drawRect = CGRect(
            x: bounds.midX - drawSize.width * 0.5 + offset.x,
            y: bounds.midY - drawSize.height * 0.5 + offset.y,
            width: drawSize.width,
            height: drawSize.height
        )
        image.draw(in: drawRect, blendMode: .normal, alpha: 1.0)
    }
}

private final class OptimizedTimelineBenchView: UIView
{
    var phase: CGFloat = 0.0
    {
        didSet
        {
            setNeedsDisplay()
        }
    }

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect)
    {
        guard let context = UIGraphicsGetCurrentContext() else
        {
            return
        }
        let count = 12
        let spacing: CGFloat = 8.0
        let barWidth = (bounds.width - spacing * CGFloat(count - 1)) / CGFloat(count)
        let maxHeight = bounds.height
        for index in 0..<count
        {
            let offset = phase * .pi * 2.0 + CGFloat(index) * 0.35
            let normalized = (sin(offset) * 0.5) + 0.5
            let height = max(12.0, maxHeight * normalized)
            let x = CGFloat(index) * (barWidth + spacing)
            let barRect = CGRect(x: x, y: maxHeight - height, width: barWidth, height: height)
            let barPath = UIBezierPath(roundedRect: barRect, cornerRadius: min(6.0, barWidth * 0.5))
            context.setFillColor(
                UIColor(
                    red: 0.20 + 0.02 * CGFloat(index),
                    green: 0.55,
                    blue: 1.0 - 0.03 * CGFloat(index),
                    alpha: 1.0
                ).cgColor
            )
            context.addPath(barPath.cgPath)
            context.fillPath()
        }
    }
}

private enum CollectionBenchMode
{
    case matrix
    case feed
    case thumbnailGrid
    case chat
}

private final class CollectionBenchCell: UICollectionViewCell
{
    static let reuseID = "CollectionBenchCell"

    private let avatarView = UIView(frame: .zero)
    private let titleBar = UIView(frame: .zero)
    private let bodyBar = UIView(frame: .zero)
    private let footerBar = UIView(frame: .zero)

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        contentView.layer.cornerRadius = 6.0
        contentView.layer.borderColor = UIColor(red: 0.16, green: 0.44, blue: 0.92, alpha: 1.0).cgColor
        [avatarView, titleBar, bodyBar, footerBar].forEach(contentView.addSubview)
        updateSelectionStyle()
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        let inset: CGFloat = 10.0
        avatarView.frame = CGRect(x: inset, y: inset, width: 28.0, height: 28.0)
        titleBar.frame = CGRect(x: inset + 38.0, y: inset + 2.0, width: max(contentView.bounds.width - 66.0, 24.0), height: 10.0)
        bodyBar.frame = CGRect(x: inset + 38.0, y: inset + 18.0, width: max(contentView.bounds.width - 84.0, 18.0), height: 9.0)
        footerBar.frame = CGRect(x: inset + 38.0, y: inset + 34.0, width: max(contentView.bounds.width - 118.0, 16.0), height: 8.0)
    }

    func configure(index: Int, mode: CollectionBenchMode)
    {
        let base = 0.90 - CGFloat(index % 5) * 0.05
        switch mode
        {
        case .matrix:
            contentView.backgroundColor = UIColor(red: base, green: 0.3, blue: 1.0 - base * 0.35, alpha: 1.0)
            avatarView.isHidden = true
            titleBar.isHidden = true
            bodyBar.isHidden = true
            footerBar.isHidden = true
        case .feed:
            contentView.backgroundColor = UIColor(red: 0.97, green: 0.98, blue: 1.0, alpha: 1.0)
            avatarView.isHidden = false
            titleBar.isHidden = false
            bodyBar.isHidden = false
            footerBar.isHidden = false
            avatarView.backgroundColor = UIColor(red: base, green: 0.42, blue: 1.0 - base * 0.28, alpha: 1.0)
            avatarView.layer.cornerRadius = 14.0
            titleBar.backgroundColor = UIColor(red: 0.22, green: 0.28, blue: 0.38, alpha: 1.0)
            bodyBar.backgroundColor = UIColor(red: 0.48, green: 0.56, blue: 0.68, alpha: 1.0)
            footerBar.backgroundColor = UIColor(red: 0.78, green: 0.82, blue: 0.90, alpha: 1.0)
        case .thumbnailGrid:
            contentView.backgroundColor = UIColor(red: base, green: 0.36, blue: 1.0 - base * 0.30, alpha: 1.0)
            avatarView.isHidden = true
            titleBar.isHidden = true
            bodyBar.isHidden = true
            footerBar.isHidden = true
        case .chat:
            let outgoing = index.isMultiple(of: 2)
            contentView.backgroundColor = .clear
            avatarView.isHidden = true
            titleBar.isHidden = false
            bodyBar.isHidden = false
            footerBar.isHidden = true
            let bubbleColor = outgoing
                ? UIColor(red: 0.24, green: 0.62, blue: 0.96, alpha: 1.0)
                : UIColor(red: 0.92, green: 0.94, blue: 0.97, alpha: 1.0)
            titleBar.backgroundColor = bubbleColor
            bodyBar.backgroundColor = outgoing
                ? UIColor(red: 0.90, green: 0.96, blue: 1.0, alpha: 1.0)
                : UIColor(red: 0.24, green: 0.30, blue: 0.38, alpha: 1.0)
        }
    }

    override var isSelected: Bool
    {
        didSet
        {
            updateSelectionStyle()
        }
    }

    private func updateSelectionStyle()
    {
        contentView.layer.borderWidth = isSelected ? 2.0 : 0.0
    }
}

private final class CollectionBenchView: UIView, UICollectionViewDataSource
{
    private let layout = UICollectionViewFlowLayout()
    private lazy var collectionView = UICollectionView(frame: .zero, collectionViewLayout: layout)
    private let mode: CollectionBenchMode

    init(frame: CGRect, mode: CollectionBenchMode = .matrix)
    {
        self.mode = mode
        super.init(frame: frame)
        layout.minimumLineSpacing = 8.0
        layout.minimumInteritemSpacing = 8.0
        collectionView.backgroundColor = .white
        collectionView.dataSource = self
        collectionView.allowsSelection = true
        collectionView.register(CollectionBenchCell.self, forCellWithReuseIdentifier: CollectionBenchCell.reuseID)
        addSubview(collectionView)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    private var itemCount: Int
    {
        switch mode
        {
        case .matrix:
            240
        case .feed:
            1_000
        case .thumbnailGrid:
            3_000
        case .chat:
            2_000
        }
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        collectionView.frame = bounds
        switch mode
        {
        case .matrix:
            let width = max((bounds.width - 24.0) / 3.0, 40.0)
            layout.itemSize = CGSize(width: width, height: width * 0.6)
        case .feed:
            layout.itemSize = CGSize(width: bounds.width - 20.0, height: 126.0)
        case .thumbnailGrid:
            let width = max((bounds.width - 24.0) / 3.0, 72.0)
            layout.itemSize = CGSize(width: width, height: width)
        case .chat:
            layout.itemSize = CGSize(width: bounds.width - 20.0, height: 68.0)
        }
    }

    func scroll(to phase: CGFloat)
    {
        let maxOffset = max(collectionView.contentSize.height - collectionView.bounds.height, 0.0)
        collectionView.contentOffset = CGPoint(x: 0, y: maxOffset * phase)
        collectionView.layoutIfNeeded()
    }

    func select(item: Int)
    {
        let clamped = max(0, min(item, itemCount - 1))
        let indexPath = IndexPath(item: clamped, section: 0)
        collectionView.selectItem(at: indexPath, animated: false, scrollPosition: .centeredVertically)
        collectionView.layoutIfNeeded()
    }

    func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int
    {
        itemCount
    }

    func collectionView(
        _ collectionView: UICollectionView,
        cellForItemAt indexPath: IndexPath
    ) -> UICollectionViewCell
    {
        let cell = collectionView.dequeueReusableCell(
            withReuseIdentifier: CollectionBenchCell.reuseID,
            for: indexPath
        ) as! CollectionBenchCell
        cell.configure(index: indexPath.item, mode: mode)
        return cell
    }
}

private final class OptimizedCollectionJourneyBenchView: UIView
{
    private let mode: CollectionBenchMode
    private var phase: CGFloat = 0.0
    private var selectedIndex: Int?
    private let selectionStrokeColor = UIColor(red: 0.16, green: 0.44, blue: 0.92, alpha: 1.0)

    init(frame: CGRect, mode: CollectionBenchMode)
    {
        self.mode = mode
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .white
        clipsToBounds = true
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func scroll(to phase: CGFloat)
    {
        self.phase = min(max(phase, 0.0), 1.0)
        selectedIndex = nil
        setNeedsDisplay()
    }

    func select(item: Int)
    {
        let clamped = max(0, min(item, itemCount - 1))
        selectedIndex = clamped
        phase = phaseForItem(clamped)
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect)
    {
        UIColor.white.setFill()
        UIRectFill(bounds)

        switch mode
        {
        case .matrix:
            drawMatrix()
        case .feed:
            drawFeed()
        case .thumbnailGrid:
            drawThumbnailGrid()
        case .chat:
            drawChat()
        }
    }

    private var itemCount: Int
    {
        switch mode
        {
        case .matrix:
            return 240
        case .feed:
            return 1_000
        case .thumbnailGrid:
            return 3_000
        case .chat:
            return 2_000
        }
    }

    private func phaseForItem(_ item: Int) -> CGFloat
    {
        let itemHeight = modeItemHeight()
        let anchorY: CGFloat
        switch mode
        {
        case .matrix, .thumbnailGrid:
            anchorY = CGFloat(item / 3) * (itemHeight + 8.0)
        case .feed, .chat:
            anchorY = CGFloat(item) * (itemHeight + 8.0)
        }

        let centered = max(anchorY - max((bounds.height - itemHeight) * 0.5, 0.0), 0.0)
        let maxOffset = max(contentHeight() - bounds.height, 0.0)
        guard maxOffset > 0.0 else
        {
            return 0.0
        }
        return min(centered / maxOffset, 1.0)
    }

    private func contentHeight() -> CGFloat
    {
        switch mode
        {
        case .matrix:
            let height = modeItemHeight()
            let rows = (itemCount + 2) / 3
            return CGFloat(rows) * height + CGFloat(max(rows - 1, 0)) * 8.0
        case .feed, .chat:
            let height = modeItemHeight()
            return CGFloat(itemCount) * height + CGFloat(max(itemCount - 1, 0)) * 8.0
        case .thumbnailGrid:
            let height = modeItemHeight()
            let rows = (itemCount + 2) / 3
            return CGFloat(rows) * height + CGFloat(max(rows - 1, 0)) * 8.0
        }
    }

    private func modeItemHeight() -> CGFloat
    {
        switch mode
        {
        case .matrix:
            let width = max((bounds.width - 24.0) / 3.0, 40.0)
            return width * 0.6
        case .feed:
            return 126.0
        case .thumbnailGrid:
            return max((bounds.width - 24.0) / 3.0, 72.0)
        case .chat:
            return 68.0
        }
    }

    private func drawMatrix()
    {
        let spacing: CGFloat = 8.0
        let cellWidth = max((bounds.width - 24.0) / 3.0, 40.0)
        let cellHeight = cellWidth * 0.6
        let rowStride = cellHeight + spacing
        let offsetY = max(contentHeight() - bounds.height, 0.0) * phase
        let rowCount = (itemCount + 2) / 3
        let startRow = max(Int(floor(offsetY / rowStride)), 0)
        let endRow = min(Int(ceil((offsetY + bounds.height) / rowStride)), rowCount - 1)
        guard endRow >= startRow else
        {
            return
        }

        for row in startRow...endRow
        {
            for column in 0..<3
            {
                let index = row * 3 + column
                if index >= itemCount
                {
                    break
                }
                let x = CGFloat(column) * (cellWidth + spacing)
                let y = CGFloat(row) * rowStride - offsetY
                let frame = CGRect(x: x, y: y, width: cellWidth, height: cellHeight)
                let path = UIBezierPath(roundedRect: frame, cornerRadius: 6.0)
                benchPaletteColor(index: index, palettePhase: row).setFill()
                path.fill()
                strokeSelectionIfNeeded(index: index, frame: frame, radius: 6.0)
            }
        }
    }

    private func drawFeed()
    {
        let spacing: CGFloat = 8.0
        let itemHeight: CGFloat = 126.0
        let stride = itemHeight + spacing
        let offsetY = max(contentHeight() - bounds.height, 0.0) * phase
        let start = max(Int(floor(offsetY / stride)), 0)
        let end = min(Int(ceil((offsetY + bounds.height) / stride)), itemCount - 1)
        guard end >= start else
        {
            return
        }

        for index in start...end
        {
            let y = CGFloat(index) * stride - offsetY
            let rowRect = CGRect(x: 10.0, y: y, width: bounds.width - 20.0, height: itemHeight)
            UIColor(red: 0.97, green: 0.98, blue: 1.0, alpha: 1.0).setFill()
            UIBezierPath(roundedRect: rowRect, cornerRadius: 6.0).fill()

            let avatarRect = CGRect(x: rowRect.minX + 10.0, y: rowRect.minY + 10.0, width: 28.0, height: 28.0)
            benchPaletteColor(index: index, palettePhase: index / 3).setFill()
            UIBezierPath(ovalIn: avatarRect).fill()

            UIColor(red: 0.22, green: 0.28, blue: 0.38, alpha: 1.0).setFill()
            UIBezierPath(
                roundedRect: CGRect(x: rowRect.minX + 48.0, y: rowRect.minY + 12.0, width: max(rowRect.width - 66.0, 24.0), height: 10.0),
                cornerRadius: 3.0
            ).fill()
            UIColor(red: 0.48, green: 0.56, blue: 0.68, alpha: 1.0).setFill()
            UIBezierPath(
                roundedRect: CGRect(x: rowRect.minX + 48.0, y: rowRect.minY + 28.0, width: max(rowRect.width - 84.0, 18.0), height: 9.0),
                cornerRadius: 3.0
            ).fill()
            UIColor(red: 0.78, green: 0.82, blue: 0.90, alpha: 1.0).setFill()
            UIBezierPath(
                roundedRect: CGRect(x: rowRect.minX + 48.0, y: rowRect.minY + 44.0, width: max(rowRect.width - 118.0, 16.0), height: 8.0),
                cornerRadius: 3.0
            ).fill()

            strokeSelectionIfNeeded(index: index, frame: rowRect, radius: 6.0)
        }
    }

    private func drawThumbnailGrid()
    {
        let spacing: CGFloat = 8.0
        let cellWidth = max((bounds.width - 24.0) / 3.0, 72.0)
        let rowStride = cellWidth + spacing
        let offsetY = max(contentHeight() - bounds.height, 0.0) * phase
        let rowCount = (itemCount + 2) / 3
        let startRow = max(Int(floor(offsetY / rowStride)), 0)
        let endRow = min(Int(ceil((offsetY + bounds.height) / rowStride)), rowCount - 1)
        guard endRow >= startRow else
        {
            return
        }

        for row in startRow...endRow
        {
            for column in 0..<3
            {
                let index = row * 3 + column
                if index >= itemCount
                {
                    break
                }
                let x = CGFloat(column) * (cellWidth + spacing)
                let y = CGFloat(row) * rowStride - offsetY
                let frame = CGRect(x: x, y: y, width: cellWidth, height: cellWidth)
                let path = UIBezierPath(roundedRect: frame, cornerRadius: 6.0)
                benchPaletteColor(index: index, palettePhase: row + column).setFill()
                path.fill()
                strokeSelectionIfNeeded(index: index, frame: frame, radius: 6.0)
            }
        }
    }

    private func drawChat()
    {
        let spacing: CGFloat = 8.0
        let itemHeight: CGFloat = 68.0
        let stride = itemHeight + spacing
        let offsetY = max(contentHeight() - bounds.height, 0.0) * phase
        let start = max(Int(floor(offsetY / stride)), 0)
        let end = min(Int(ceil((offsetY + bounds.height) / stride)), itemCount - 1)
        guard end >= start else
        {
            return
        }

        for index in start...end
        {
            let y = CGFloat(index) * stride - offsetY
            let rowRect = CGRect(x: 10.0, y: y, width: bounds.width - 20.0, height: itemHeight)
            let outgoing = index.isMultiple(of: 2)
            let bubbleWidth = max(rowRect.width - 84.0, 80.0)
            let bubbleX = outgoing ? rowRect.maxX - bubbleWidth - 12.0 : rowRect.minX + 12.0
            let bubbleRect = CGRect(x: bubbleX, y: rowRect.minY + 10.0, width: bubbleWidth, height: 22.0)
            let detailWidth = max(bubbleWidth - 18.0, 36.0)
            let detailRect = CGRect(x: bubbleX, y: rowRect.minY + 38.0, width: detailWidth, height: 9.0)

            let titleColor = outgoing
                ? UIColor(red: 0.24, green: 0.62, blue: 0.96, alpha: 1.0)
                : UIColor(red: 0.92, green: 0.94, blue: 0.97, alpha: 1.0)
            let detailColor = outgoing
                ? UIColor(red: 0.90, green: 0.96, blue: 1.0, alpha: 1.0)
                : UIColor(red: 0.24, green: 0.30, blue: 0.38, alpha: 1.0)

            titleColor.setFill()
            UIBezierPath(roundedRect: bubbleRect, cornerRadius: 10.0).fill()
            detailColor.setFill()
            UIBezierPath(roundedRect: detailRect, cornerRadius: 4.0).fill()

            strokeSelectionIfNeeded(index: index, frame: rowRect, radius: 6.0)
        }
    }

    private func strokeSelectionIfNeeded(index: Int, frame: CGRect, radius: CGFloat)
    {
        guard selectedIndex == index else
        {
            return
        }
        selectionStrokeColor.setStroke()
        let path = UIBezierPath(roundedRect: frame.insetBy(dx: 1.0, dy: 1.0), cornerRadius: radius)
        path.lineWidth = 2.0
        path.stroke()
    }
}

private func benchPaletteColor(index: Int, palettePhase: Int) -> UIColor
{
    switch (index + palettePhase) % 6
    {
    case 0:
        UIColor(red: 0.18, green: 0.48, blue: 0.96, alpha: 1.0)
    case 1:
        UIColor(red: 0.96, green: 0.38, blue: 0.24, alpha: 1.0)
    case 2:
        UIColor(red: 0.22, green: 0.72, blue: 0.42, alpha: 1.0)
    case 3:
        UIColor(red: 0.96, green: 0.74, blue: 0.18, alpha: 1.0)
    case 4:
        UIColor(red: 0.58, green: 0.38, blue: 0.96, alpha: 1.0)
    default:
        UIColor(red: 0.16, green: 0.68, blue: 0.86, alpha: 1.0)
    }
}

private final class InsetGridBenchView: UIView
{
    let gridView = FlatRectGridBenchView(frame: .zero)
    var contentInsets = UIEdgeInsets(top: 8.0, left: 8.0, bottom: 8.0, right: 8.0)

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        addSubview(gridView)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        gridView.frame = bounds.inset(by: contentInsets)
        gridView.layoutIfNeeded()
    }
}

private final class DeepStackBenchView: UIView
{
    private var nodes: [UIView] = []
    private var contentInset: CGFloat = 12.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.98, green: 0.99, blue: 1.0, alpha: 1.0)
        var parent: UIView = self
        for depth in 0..<30
        {
            let node = UIView(frame: .zero)
            node.backgroundColor = benchPaletteColor(index: depth, palettePhase: 0)
            node.layer.cornerRadius = 8.0
            parent.addSubview(node)
            nodes.append(node)
            parent = node
        }
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        var rect = bounds.insetBy(dx: contentInset, dy: contentInset)
        for node in nodes
        {
            node.frame = CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: max(rect.height, 24.0))
            rect = node.bounds.insetBy(dx: 6.0, dy: 6.0)
        }
    }

    func runThemeSwap(step: Int)
    {
        contentInset = step.isMultiple(of: 2) ? 12.0 : 22.0
        for (index, node) in nodes.enumerated()
        {
            node.backgroundColor = benchPaletteColor(index: index, palettePhase: step)
        }
        setNeedsLayout()
        layoutIfNeeded()
    }
}

private final class LargeEditorBenchView: UIView
{
    let textView = UITextView(frame: .zero)

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.96, green: 0.97, blue: 1.0, alpha: 1.0)
        textView.backgroundColor = .white
        textView.font = .systemFont(ofSize: 15.0)
        textView.textColor = UIColor(red: 0.12, green: 0.16, blue: 0.24, alpha: 1.0)
        textView.layer.cornerRadius = 12.0
        addSubview(textView)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        textView.frame = bounds.insetBy(dx: 12.0, dy: 12.0)
    }

    func seedText(lines: Int) -> String
    {
        var output = ""
        for line in 0..<lines
        {
            output += "Orbit \(line % 17) telemetry line \(line) retains enough prose to force multiline wrapping.\n"
        }
        return output
    }

    func runKeystrokeBurst(step: Int)
    {
        textView.text = seedText(lines: 96)
        textView.selectedRange = NSRange(location: textView.text.count, length: 0)
        withPerfSignpost("text.measure")
        {
            for chunk in 0..<32
            {
                let insertion = chunk.isMultiple(of: 4) ? "\npatch" : " patch"
                let cursor = textView.selectedRange.location
                let nsText = textView.text as NSString
                let next = nsText.replacingCharacters(in: NSRange(location: cursor, length: 0), with: insertion)
                textView.text = next
                textView.selectedRange = NSRange(location: cursor + insertion.count, length: 0)
            }
            if step.isMultiple(of: 2) && !textView.text.isEmpty
            {
                textView.text.removeLast()
            }
        }
    }

    func runPaste()
    {
        textView.text = seedText(lines: 64)
        let paste = String(repeating: "paste-block ", count: 860)
        textView.selectedRange = NSRange(location: 48, length: 80)
        withPerfSignpost("text.measure")
        {
            let next = (textView.text as NSString).replacingCharacters(in: textView.selectedRange, with: paste)
            textView.text = next
            textView.selectedRange = NSRange(location: 48 + paste.count, length: 0)
        }
    }

    func runSelectionReplace()
    {
        textView.text = seedText(lines: 128)
        textView.selectedRange = NSRange(location: 120, length: 140)
        withPerfSignpost("text.measure")
        {
            let next = (textView.text as NSString).replacingCharacters(
                in: textView.selectedRange,
                with: "[selection replaced]"
            )
            textView.text = next
            textView.selectedRange = NSRange(location: 120 + 20, length: 0)
        }
    }
}

private final class StressBarsBenchView: UIView
{
    private var bars: [CALayer] = []

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.97, green: 0.98, blue: 1.0, alpha: 1.0)
        for _ in 0..<300
        {
            let layer = CALayer()
            layer.cornerRadius = 6.0
            bars.append(layer)
            self.layer.addSublayer(layer)
        }
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        runPhase(step: 0)
    }

    func runPhase(step: Int)
    {
        let columns = 20
        for (index, bar) in bars.enumerated()
        {
            let row = index / columns
            let column = index % columns
            let phase = CGFloat((index + step) % 23) / 23.0
            let height = 10.0 + phase * 38.0
            let x = CGFloat(column) * 18.0
            let y = CGFloat(row) * 22.0 + (48.0 - height)
            bar.frame = CGRect(x: x, y: y, width: 12.0, height: height)
            bar.backgroundColor = benchPaletteColor(index: index, palettePhase: step).cgColor
        }
    }
}

private func gridFrameForIndex(
    _ index: Int,
    cellSize: CGSize,
    spacing: CGFloat,
    boundsWidth: CGFloat
) -> CGRect
{
    let cols = max(Int((boundsWidth + spacing) / (cellSize.width + spacing)), 1)
    let row = index / cols
    let col = index % cols
    return CGRect(
        x: CGFloat(col) * (cellSize.width + spacing),
        y: CGFloat(row) * (cellSize.height + spacing),
        width: cellSize.width,
        height: cellSize.height
    )
}

private final class FlatRectGridBenchView: UIView
{
    private var rectViews: [UIView] = []
    private let cellSize = CGSize(width: 28.0, height: 18.0)
    private var spacing: CGFloat = 6.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        clipsToBounds = true
        backgroundColor = .clear
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        for (index, view) in rectViews.enumerated()
        {
            view.frame = gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
        }
    }

    func install(count: Int, palettePhase: Int)
    {
        removeAllRects()
        rectViews.reserveCapacity(count)
        for index in 0..<count
        {
            let view = UIView(
                frame: gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
            )
            view.backgroundColor = Self.fillColor(index: index, palettePhase: palettePhase)
            view.alpha = 0.90
            view.layer.cornerRadius = 4.0
            addSubview(view)
            rectViews.append(view)
        }
        setNeedsLayout()
        layoutIfNeeded()
    }

    func mutate(palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            for (index, view) in rectViews.enumerated()
            {
                view.backgroundColor = Self.fillColor(index: index, palettePhase: palettePhase)
                view.alpha = 0.72 + CGFloat((index + palettePhase) % 5) * 0.05
            }
        }
    }

    func mutateSubset(limit: Int, palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            let upperBound = min(limit, rectViews.count)
            guard upperBound > 0 else
            {
                return
            }
            for index in 0..<upperBound
            {
                let view = rectViews[index]
                view.backgroundColor = Self.fillColor(index: index, palettePhase: palettePhase)
                view.alpha = 0.72 + CGFloat((index + palettePhase) % 5) * 0.05
            }
        }
    }

    func runThemeSwap(step: Int)
    {
        withPerfSignpost("diff.apply")
        {
            spacing = step.isMultiple(of: 2) ? 10.0 : 4.0
            backgroundColor = step.isMultiple(of: 2)
                ? UIColor(red: 0.96, green: 0.98, blue: 1.0, alpha: 1.0)
                : UIColor(red: 0.92, green: 0.95, blue: 0.99, alpha: 1.0)
            for (index, view) in rectViews.enumerated()
            {
                view.backgroundColor = Self.fillColor(index: index, palettePhase: step)
                view.alpha = 0.68 + CGFloat((index + step) % 4) * 0.06
                view.layer.cornerRadius = step.isMultiple(of: 2) ? 6.0 : 3.0
            }
        }
        setNeedsLayout()
        layoutIfNeeded()
    }

    func removeAllRects()
    {
        rectViews.forEach { $0.removeFromSuperview() }
        rectViews.removeAll(keepingCapacity: true)
    }

    private static func fillColor(index: Int, palettePhase: Int) -> UIColor
    {
        switch (index + palettePhase) % 6
        {
        case 0:
            return UIColor(red: 0.18, green: 0.48, blue: 0.96, alpha: 1.0)
        case 1:
            return UIColor(red: 0.96, green: 0.38, blue: 0.24, alpha: 1.0)
        case 2:
            return UIColor(red: 0.22, green: 0.72, blue: 0.42, alpha: 1.0)
        case 3:
            return UIColor(red: 0.96, green: 0.74, blue: 0.18, alpha: 1.0)
        case 4:
            return UIColor(red: 0.58, green: 0.38, blue: 0.96, alpha: 1.0)
        default:
            return UIColor(red: 0.16, green: 0.68, blue: 0.86, alpha: 1.0)
        }
    }
}

private final class OptimizedFlatRectGridBenchView: UIView
{
    private var count = 0
    private var palettePhase = 0
    private let cellSize = CGSize(width: 28.0, height: 18.0)
    private let spacing: CGFloat = 6.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        clipsToBounds = true
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func install(count: Int, palettePhase: Int)
    {
        self.count = count
        self.palettePhase = palettePhase
        setNeedsDisplay()
    }

    func mutate(palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            self.palettePhase = palettePhase
            setNeedsDisplay()
        }
    }

    override func draw(_ rect: CGRect)
    {
        guard let context = UIGraphicsGetCurrentContext() else
        {
            return
        }
        for index in 0..<count
        {
            let frame = gridFrameForIndex(
                index,
                cellSize: cellSize,
                spacing: spacing,
                boundsWidth: bounds.width
            )
            let alpha = 0.72 + CGFloat((index + palettePhase) % 5) * 0.05
            context.setFillColor(Self.fillColor(index: index, palettePhase: palettePhase).withAlphaComponent(alpha).cgColor)
            let path = UIBezierPath(roundedRect: frame, cornerRadius: 4.0)
            context.addPath(path.cgPath)
            context.fillPath()
        }
    }

    private static func fillColor(index: Int, palettePhase: Int) -> UIColor
    {
        switch (index + palettePhase) % 6
        {
        case 0:
            return UIColor(red: 0.18, green: 0.48, blue: 0.96, alpha: 1.0)
        case 1:
            return UIColor(red: 0.96, green: 0.38, blue: 0.24, alpha: 1.0)
        case 2:
            return UIColor(red: 0.22, green: 0.72, blue: 0.42, alpha: 1.0)
        case 3:
            return UIColor(red: 0.96, green: 0.74, blue: 0.18, alpha: 1.0)
        case 4:
            return UIColor(red: 0.58, green: 0.38, blue: 0.96, alpha: 1.0)
        default:
            return UIColor(red: 0.16, green: 0.68, blue: 0.86, alpha: 1.0)
        }
    }
}

private final class CardGridBenchItemView: UIView
{
    private let fillView = UIView()

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        addSubview(fillView)
        layer.cornerRadius = 12.0
        layer.shadowColor = UIColor.black.cgColor
        layer.shadowOpacity = 0.12
        layer.shadowRadius = 8.0
        layer.shadowOffset = CGSize(width: 0, height: 4)
        layer.borderWidth = 1.5
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        fillView.frame = bounds.insetBy(dx: 1.5, dy: 1.5)
        fillView.layer.cornerRadius = 10.5
    }

    func applyPalette(index: Int, palettePhase: Int)
    {
        layer.borderColor = Self.borderColor(index: index, palettePhase: palettePhase).cgColor
        fillView.backgroundColor = Self.fillColor(index: index, palettePhase: palettePhase)
    }

    private static func borderColor(index: Int, palettePhase: Int) -> UIColor
    {
        switch (index + palettePhase) % 4
        {
        case 0:
            return UIColor(red: 0.90, green: 0.92, blue: 0.96, alpha: 1.0)
        case 1:
            return UIColor(red: 0.78, green: 0.84, blue: 0.94, alpha: 1.0)
        case 2:
            return UIColor(red: 0.90, green: 0.82, blue: 0.78, alpha: 1.0)
        default:
            return UIColor(red: 0.82, green: 0.90, blue: 0.86, alpha: 1.0)
        }
    }

    private static func fillColor(index: Int, palettePhase: Int) -> UIColor
    {
        switch (index + palettePhase) % 5
        {
        case 0:
            return UIColor(red: 0.96, green: 0.97, blue: 1.0, alpha: 1.0)
        case 1:
            return UIColor(red: 0.92, green: 0.96, blue: 1.0, alpha: 1.0)
        case 2:
            return UIColor(red: 1.0, green: 0.95, blue: 0.92, alpha: 1.0)
        case 3:
            return UIColor(red: 0.94, green: 1.0, blue: 0.95, alpha: 1.0)
        default:
            return UIColor(red: 0.97, green: 0.94, blue: 1.0, alpha: 1.0)
        }
    }
}

private final class CardGridBenchView: UIView
{
    private var cardViews: [CardGridBenchItemView] = []
    private let cellSize = CGSize(width: 76.0, height: 52.0)
    private let spacing: CGFloat = 12.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        clipsToBounds = true
        backgroundColor = .clear
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        for (index, view) in cardViews.enumerated()
        {
            view.frame = gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
        }
    }

    func install(count: Int, palettePhase: Int)
    {
        clear()
        cardViews.reserveCapacity(count)
        for index in 0..<count
        {
            let view = CardGridBenchItemView(
                frame: gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
            )
            view.applyPalette(index: index, palettePhase: palettePhase)
            addSubview(view)
            cardViews.append(view)
        }
        setNeedsLayout()
        layoutIfNeeded()
    }

    func mutate(palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            for (index, view) in cardViews.enumerated()
            {
                view.applyPalette(index: index, palettePhase: palettePhase)
            }
        }
    }

    private func clear()
    {
        cardViews.forEach { $0.removeFromSuperview() }
        cardViews.removeAll(keepingCapacity: true)
    }
}

private final class LabelGridBenchView: UIView
{
    private var labels: [UILabel] = []
    private let cellSize = CGSize(width: 92.0, height: 34.0)
    private let spacing: CGFloat = 8.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        clipsToBounds = true
        backgroundColor = .clear
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        for (index, label) in labels.enumerated()
        {
            label.frame = gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
        }
    }

    func install(count: Int, palettePhase: Int)
    {
        clear()
        labels.reserveCapacity(count)
        for index in 0..<count
        {
            let label = UILabel(
                frame: gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
            )
            label.numberOfLines = 2
            label.font = .systemFont(ofSize: 13.0)
            configure(label: label, index: index, palettePhase: palettePhase)
            addSubview(label)
            labels.append(label)
        }
        setNeedsLayout()
        layoutIfNeeded()
    }

    func mutate(palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            for (index, label) in labels.enumerated()
            {
                configure(label: label, index: index, palettePhase: palettePhase)
            }
        }
        withPerfSignpost("text.measure")
        {
            for label in labels
            {
                _ = label.sizeThatFits(CGSize(width: cellSize.width, height: cellSize.height * 2.0))
            }
        }
    }

    private func clear()
    {
        labels.forEach { $0.removeFromSuperview() }
        labels.removeAll(keepingCapacity: true)
    }

    private func configure(label: UILabel, index: Int, palettePhase: Int)
    {
        label.text = Self.text(index: index, palettePhase: palettePhase)
        label.textColor = Self.textColor(index: index, palettePhase: palettePhase)
    }

    private static func text(index: Int, palettePhase: Int) -> String
    {
        if (index + palettePhase) % 3 == 0
        {
            return "Oxide \(index % 97) status \(palettePhase % 11)"
        }
        return "Pilot \((index + palettePhase) % 257) ready"
    }

    private static func textColor(index: Int, palettePhase: Int) -> UIColor
    {
        switch (index + palettePhase) % 4
        {
        case 0:
            return UIColor(red: 0.10, green: 0.12, blue: 0.18, alpha: 1.0)
        case 1:
            return UIColor(red: 0.18, green: 0.30, blue: 0.58, alpha: 1.0)
        case 2:
            return UIColor(red: 0.62, green: 0.22, blue: 0.20, alpha: 1.0)
        default:
            return UIColor(red: 0.14, green: 0.44, blue: 0.32, alpha: 1.0)
        }
    }
}

private final class ImageGridBenchView: UIView
{
    private let image: UIImage
    private var imageViews: [UIImageView] = []
    private let cellSize = CGSize(width: 84.0, height: 64.0)
    private let spacing: CGFloat = 10.0

    init(frame: CGRect, image: UIImage)
    {
        self.image = image
        super.init(frame: frame)
        clipsToBounds = true
        backgroundColor = .clear
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        for (index, imageView) in imageViews.enumerated()
        {
            imageView.frame = gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
        }
    }

    func install(count: Int, palettePhase: Int)
    {
        clear()
        imageViews.reserveCapacity(count)
        for index in 0..<count
        {
            let imageView = UIImageView(
                frame: gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
            )
            imageView.image = image
            configure(imageView: imageView, index: index, palettePhase: palettePhase)
            addSubview(imageView)
            imageViews.append(imageView)
        }
        setNeedsLayout()
        layoutIfNeeded()
    }

    func mutate(palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            for (index, imageView) in imageViews.enumerated()
            {
                configure(imageView: imageView, index: index, palettePhase: palettePhase)
            }
        }
    }

    private func clear()
    {
        imageViews.forEach { $0.removeFromSuperview() }
        imageViews.removeAll(keepingCapacity: true)
    }

    private func configure(imageView: UIImageView, index: Int, palettePhase: Int)
    {
        let even = (index + palettePhase) % 2 == 0
        imageView.alpha = even ? 1.0 : 0.62
        imageView.contentMode = even ? .scaleAspectFit : .scaleAspectFill
        imageView.clipsToBounds = true
        imageView.layer.cornerRadius = 8.0
    }
}

private final class OptimizedLabelGridBenchView: UIView
{
    private var count = 0
    private var palettePhase = 0
    private let cellSize = CGSize(width: 92.0, height: 34.0)
    private let spacing: CGFloat = 8.0
    private let font = UIFont.systemFont(ofSize: 13.0)
    private let paragraphStyle: NSParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byWordWrapping
        return style.copy() as! NSParagraphStyle
    }()

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        clipsToBounds = true
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func install(count: Int, palettePhase: Int)
    {
        self.count = count
        self.palettePhase = palettePhase
        setNeedsDisplay()
    }

    func mutate(palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            self.palettePhase = palettePhase
            setNeedsDisplay()
        }
    }

    override func draw(_ rect: CGRect)
    {
        let options: NSStringDrawingOptions = [.usesLineFragmentOrigin, .usesFontLeading]
        for index in 0..<count
        {
            let frame = gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
            let text = Self.text(index: index, palettePhase: palettePhase)
            let attributes: [NSAttributedString.Key: Any] = [
                .font: font,
                .foregroundColor: Self.textColor(index: index, palettePhase: palettePhase),
                .paragraphStyle: paragraphStyle,
            ]
            (text as NSString).draw(
                with: frame,
                options: options,
                attributes: attributes,
                context: nil
            )
        }
    }

    private static func text(index: Int, palettePhase: Int) -> String
    {
        if (index + palettePhase) % 3 == 0
        {
            return "Oxide \(index % 97) status \(palettePhase % 11)"
        }
        return "Pilot \((index + palettePhase) % 257) ready"
    }

    private static func textColor(index: Int, palettePhase: Int) -> UIColor
    {
        switch (index + palettePhase) % 4
        {
        case 0:
            return UIColor(red: 0.10, green: 0.12, blue: 0.18, alpha: 1.0)
        case 1:
            return UIColor(red: 0.18, green: 0.30, blue: 0.58, alpha: 1.0)
        case 2:
            return UIColor(red: 0.62, green: 0.22, blue: 0.20, alpha: 1.0)
        default:
            return UIColor(red: 0.14, green: 0.44, blue: 0.32, alpha: 1.0)
        }
    }
}

private final class OptimizedCardGridBenchView: UIView
{
    private var count = 0
    private var palettePhase = 0
    private let cellSize = CGSize(width: 76.0, height: 52.0)
    private let spacing: CGFloat = 12.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        clipsToBounds = true
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func install(count: Int, palettePhase: Int)
    {
        self.count = count
        self.palettePhase = palettePhase
        setNeedsDisplay()
    }

    func mutate(palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            self.palettePhase = palettePhase
            setNeedsDisplay()
        }
    }

    override func draw(_ rect: CGRect)
    {
        guard let context = UIGraphicsGetCurrentContext() else
        {
            return
        }
        for index in 0..<count
        {
            let frame = gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
            let inner = frame.insetBy(dx: 1.5, dy: 1.5)
            let outerPath = UIBezierPath(roundedRect: frame, cornerRadius: 12.0)
            let innerPath = UIBezierPath(roundedRect: inner, cornerRadius: 10.5)
            let fill = Self.fillColor(index: index, palettePhase: palettePhase).cgColor
            let border = Self.borderColor(index: index, palettePhase: palettePhase).cgColor

            context.saveGState()
            context.setShadow(
                offset: CGSize(width: 0.0, height: 4.0),
                blur: 8.0,
                color: UIColor.black.withAlphaComponent(0.12).cgColor
            )
            context.setFillColor(fill)
            context.addPath(innerPath.cgPath)
            context.fillPath()
            context.restoreGState()

            context.setFillColor(fill)
            context.addPath(innerPath.cgPath)
            context.fillPath()
            context.setStrokeColor(border)
            context.setLineWidth(1.5)
            context.addPath(outerPath.cgPath)
            context.strokePath()
        }
    }

    private static func borderColor(index: Int, palettePhase: Int) -> UIColor
    {
        switch (index + palettePhase) % 4
        {
        case 0:
            return UIColor(red: 0.90, green: 0.92, blue: 0.96, alpha: 1.0)
        case 1:
            return UIColor(red: 0.78, green: 0.84, blue: 0.94, alpha: 1.0)
        case 2:
            return UIColor(red: 0.90, green: 0.82, blue: 0.78, alpha: 1.0)
        default:
            return UIColor(red: 0.82, green: 0.90, blue: 0.86, alpha: 1.0)
        }
    }

    private static func fillColor(index: Int, palettePhase: Int) -> UIColor
    {
        switch (index + palettePhase) % 5
        {
        case 0:
            return UIColor(red: 0.96, green: 0.97, blue: 1.0, alpha: 1.0)
        case 1:
            return UIColor(red: 0.92, green: 0.96, blue: 1.0, alpha: 1.0)
        case 2:
            return UIColor(red: 1.0, green: 0.95, blue: 0.92, alpha: 1.0)
        case 3:
            return UIColor(red: 0.94, green: 1.0, blue: 0.95, alpha: 1.0)
        default:
            return UIColor(red: 0.97, green: 0.94, blue: 1.0, alpha: 1.0)
        }
    }
}

private final class OptimizedImageGridBenchView: UIView
{
    private let image: UIImage
    private var count = 0
    private var palettePhase = 0
    private let cellSize = CGSize(width: 84.0, height: 64.0)
    private let spacing: CGFloat = 10.0

    init(frame: CGRect, image: UIImage)
    {
        self.image = image
        super.init(frame: frame)
        isOpaque = false
        clipsToBounds = true
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func install(count: Int, palettePhase: Int)
    {
        self.count = count
        self.palettePhase = palettePhase
        setNeedsDisplay()
    }

    func mutate(palettePhase: Int)
    {
        withPerfSignpost("diff.apply")
        {
            self.palettePhase = palettePhase
            setNeedsDisplay()
        }
    }

    override func draw(_ rect: CGRect)
    {
        guard let context = UIGraphicsGetCurrentContext() else
        {
            return
        }
        for index in 0..<count
        {
            let frame = gridFrameForIndex(index, cellSize: cellSize, spacing: spacing, boundsWidth: bounds.width)
            let even = (index + palettePhase) % 2 == 0
            let path = UIBezierPath(roundedRect: frame, cornerRadius: 8.0)
            let drawRect = Self.imageRect(
                imageSize: image.size,
                in: frame,
                fit: even ? .scaleAspectFit : .scaleAspectFill
            )
            context.saveGState()
            path.addClip()
            image.draw(in: drawRect, blendMode: .normal, alpha: even ? 1.0 : 0.62)
            context.restoreGState()
        }
    }

    private static func imageRect(
        imageSize: CGSize,
        in frame: CGRect,
        fit: UIView.ContentMode
    ) -> CGRect
    {
        guard imageSize.width > 0.0 && imageSize.height > 0.0 else
        {
            return frame
        }
        let widthScale = frame.width / imageSize.width
        let heightScale = frame.height / imageSize.height
        let scale = fit == .scaleAspectFill ? max(widthScale, heightScale) : min(widthScale, heightScale)
        let size = CGSize(width: imageSize.width * scale, height: imageSize.height * scale)
        return CGRect(
            x: frame.midX - size.width * 0.5,
            y: frame.midY - size.height * 0.5,
            width: size.width,
            height: size.height
        )
    }
}

private final class OptimizedTextListBenchView: UIView
{
    private var lines: [String] = []
    private var accentPhase = 0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func install(lines: [String], accentPhase: Int)
    {
        self.lines = lines
        self.accentPhase = accentPhase
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect)
    {
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.lineBreakMode = .byTruncatingTail
        let attributes: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 13.0, weight: .medium),
            .foregroundColor: UIColor(red: 0.14, green: 0.18, blue: 0.24, alpha: 1.0),
            .paragraphStyle: paragraphStyle,
        ]
        for (index, line) in lines.enumerated()
        {
            let y = 12.0 + CGFloat(index) * 20.0
            if y + 18.0 > bounds.height
            {
                break
            }
            let rowRect = CGRect(x: 14.0, y: y, width: bounds.width - 28.0, height: 18.0)
            let accentRect = CGRect(x: rowRect.minX, y: rowRect.minY + 3.0, width: 6.0, height: 12.0)
            benchPaletteColor(index: index, palettePhase: accentPhase).setFill()
            UIBezierPath(roundedRect: accentRect, cornerRadius: 3.0).fill()
            (line as NSString).draw(
                in: rowRect.insetBy(dx: 14.0, dy: 0.0),
                withAttributes: attributes
            )
        }
    }
}

private final class OptimizedEditorBenchView: UIView
{
    private let textInset = UIEdgeInsets(top: 18.0, left: 18.0, bottom: 18.0, right: 18.0)
    private var editorText = ""
    private var selection = NSRange(location: 0, length: 0)

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = UIColor(red: 0.96, green: 0.97, blue: 1.0, alpha: 1.0)
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func seedText(lines: Int) -> String
    {
        var output = ""
        for line in 0..<lines
        {
            output += "Orbit \(line % 17) telemetry line \(line) retains enough prose to force multiline wrapping.\n"
        }
        return output
    }

    func runKeystrokeBurst(step: Int)
    {
        editorText = seedText(lines: 96)
        selection = NSRange(location: (editorText as NSString).length, length: 0)
        withPerfSignpost("text.measure")
        {
            for chunk in 0..<32
            {
                let insertion = chunk.isMultiple(of: 4) ? "\npatch" : " patch"
                let next = (editorText as NSString).replacingCharacters(
                    in: NSRange(location: selection.location, length: 0),
                    with: insertion
                )
                editorText = next
                selection = NSRange(location: selection.location + insertion.count, length: 0)
            }
            if step.isMultiple(of: 2) && !editorText.isEmpty
            {
                editorText.removeLast()
                selection = NSRange(location: max((editorText as NSString).length - 1, 0), length: 0)
            }
            _ = (editorText as NSString).boundingRect(
                with: textRect().size,
                options: [.usesLineFragmentOrigin, .usesFontLeading],
                attributes: textAttributes(),
                context: nil
            )
        }
        setNeedsDisplay()
    }

    func runPaste()
    {
        editorText = seedText(lines: 64)
        let paste = String(repeating: "paste-block ", count: 860)
        selection = NSRange(location: 48, length: 80)
        withPerfSignpost("text.measure")
        {
            editorText = (editorText as NSString).replacingCharacters(in: selection, with: paste)
            selection = NSRange(location: 48 + paste.count, length: 0)
            _ = (editorText as NSString).boundingRect(
                with: textRect().size,
                options: [.usesLineFragmentOrigin, .usesFontLeading],
                attributes: textAttributes(),
                context: nil
            )
        }
        setNeedsDisplay()
    }

    func runSelectionReplace()
    {
        editorText = seedText(lines: 128)
        selection = NSRange(location: 120, length: 140)
        withPerfSignpost("text.measure")
        {
            editorText = (editorText as NSString).replacingCharacters(
                in: selection,
                with: "[selection replaced]"
            )
            selection = NSRange(location: 140, length: 20)
            _ = (editorText as NSString).boundingRect(
                with: textRect().size,
                options: [.usesLineFragmentOrigin, .usesFontLeading],
                attributes: textAttributes(),
                context: nil
            )
        }
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect)
    {
        let panelRect = bounds.insetBy(dx: 12.0, dy: 12.0)
        UIColor.white.setFill()
        UIBezierPath(roundedRect: panelRect, cornerRadius: 12.0).fill()

        let textRect = textRect()
        let nsText = editorText as NSString
        let totalUnits = max(nsText.length, 1)
        let maxVisibleLines = max(Int(textRect.height / 18.0), 1)
        let fraction = CGFloat(min(selection.location, totalUnits)) / CGFloat(totalUnits)
        let lineIndex = min(max(Int(CGFloat(maxVisibleLines) * fraction), 0), maxVisibleLines - 1)
        let highlightRect = CGRect(
            x: textRect.minX,
            y: textRect.minY + CGFloat(lineIndex) * 18.0,
            width: textRect.width,
            height: 18.0
        )
        UIColor(red: 0.84, green: 0.92, blue: 1.0, alpha: selection.length > 0 ? 0.80 : 0.32).setFill()
        UIBezierPath(roundedRect: highlightRect, cornerRadius: 6.0).fill()

        nsText.draw(
            with: textRect,
            options: [.usesLineFragmentOrigin, .usesFontLeading],
            attributes: textAttributes(),
            context: nil
        )
    }

    private func textRect() -> CGRect
    {
        bounds.inset(by: textInset).insetBy(dx: 6.0, dy: 6.0)
    }

    private func textAttributes() -> [NSAttributedString.Key: Any]
    {
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.lineBreakMode = .byWordWrapping
        return [
            .font: UIFont.systemFont(ofSize: 15.0),
            .foregroundColor: UIColor(red: 0.12, green: 0.16, blue: 0.24, alpha: 1.0),
            .paragraphStyle: paragraphStyle,
        ]
    }
}

private final class OptimizedFormJourneyBenchView: UIView
{
    private let roles = [
        "Conference Pass",
        "Guest Access",
        "Team Member",
        "Partner",
        "Moderator",
    ]
    private var username = "Pilot"
    private var password = "Orbit123"
    private var status = "Ready"
    private var selectedRoleIndex = 0
    private var buttonScale: CGFloat = 1.0

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = UIColor(red: 0.95, green: 0.97, blue: 1.0, alpha: 1.0)
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func runJourney(step: Int)
    {
        withPerfSignpost("diff.apply")
        {
            username = "Pilot \(step % 7)"
            password = "Orbit12\(step % 10)"
            selectedRoleIndex = step % roles.count
            status = "Welcome Pilot \(step % 7)! Assigned role: \(roles[selectedRoleIndex])"
        }
        withPerfSignpost("first.interactive")
        {
            buttonScale = step.isMultiple(of: 2) ? 0.985 : 1.0
        }
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect)
    {
        drawField(
            CGRect(x: 18.0, y: 32.0, width: 250.0, height: 44.0),
            title: "Display name",
            value: username
        )
        drawField(
            CGRect(x: 18.0, y: 92.0, width: 250.0, height: 44.0),
            title: "Password",
            value: String(repeating: "•", count: max(password.count, 1))
        )

        let buttonRect = CGRect(x: 18.0, y: 152.0, width: 190.0, height: 44.0)
        let insetX = buttonRect.width * (1.0 - buttonScale) * 0.5
        let insetY = buttonRect.height * (1.0 - buttonScale) * 0.5
        let scaledButtonRect = buttonRect.insetBy(dx: insetX, dy: insetY)
        UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0).setFill()
        UIBezierPath(roundedRect: scaledButtonRect, cornerRadius: 12.0).fill()
        let buttonAttrs: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 16.0, weight: .semibold),
            .foregroundColor: UIColor.white,
        ]
        ("Create Mission" as NSString).draw(
            in: scaledButtonRect.insetBy(dx: 18.0, dy: 11.0),
            withAttributes: buttonAttrs
        )

        let pickerRect = CGRect(x: bounds.width - 230.0, y: 24.0, width: 212.0, height: 180.0)
        UIColor.white.setFill()
        UIBezierPath(roundedRect: pickerRect, cornerRadius: 14.0).fill()
        for (index, role) in roles.enumerated()
        {
            let rowRect = CGRect(x: pickerRect.minX + 12.0, y: pickerRect.minY + 16.0 + CGFloat(index) * 28.0, width: pickerRect.width - 24.0, height: 22.0)
            if index == selectedRoleIndex
            {
                UIColor(red: 0.84, green: 0.92, blue: 1.0, alpha: 0.85).setFill()
                UIBezierPath(roundedRect: rowRect, cornerRadius: 8.0).fill()
            }
            let roleAttrs: [NSAttributedString.Key: Any] = [
                .font: UIFont.systemFont(ofSize: 13.0, weight: index == selectedRoleIndex ? .semibold : .regular),
                .foregroundColor: UIColor(red: 0.20, green: 0.22, blue: 0.28, alpha: 1.0),
            ]
            (role as NSString).draw(in: rowRect.insetBy(dx: 10.0, dy: 2.0), withAttributes: roleAttrs)
        }

        let statusRect = CGRect(x: 18.0, y: 214.0, width: bounds.width - 36.0, height: 42.0)
        let statusAttrs: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 13.0),
            .foregroundColor: UIColor(white: 0.28, alpha: 1.0),
        ]
        (status as NSString).draw(with: statusRect, options: [.usesLineFragmentOrigin], attributes: statusAttrs, context: nil)
    }

    private func drawField(_ rect: CGRect, title: String, value: String)
    {
        UIColor.white.setFill()
        UIBezierPath(roundedRect: rect, cornerRadius: 12.0).fill()
        let titleAttrs: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 11.0, weight: .medium),
            .foregroundColor: UIColor(red: 0.44, green: 0.48, blue: 0.58, alpha: 1.0),
        ]
        let valueAttrs: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 15.0),
            .foregroundColor: UIColor(red: 0.12, green: 0.16, blue: 0.24, alpha: 1.0),
        ]
        (title as NSString).draw(in: rect.insetBy(dx: 14.0, dy: 6.0), withAttributes: titleAttrs)
        let valueRect = CGRect(x: rect.minX + 14.0, y: rect.minY + 19.0, width: rect.width - 28.0, height: 18.0)
        (value as NSString).draw(in: valueRect, withAttributes: valueAttrs)
    }
}

private final class OptimizedOrchestrationBenchView: UIView
{
    private var phase: CGFloat = 0.0
    private var showingModal = false

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = UIColor(red: 0.97, green: 0.98, blue: 1.0, alpha: 1.0)
        contentScaleFactor = UIScreen.main.scale
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    func runJourney(step: Int)
    {
        withPerfSignpost("transition")
        {
            phase = CGFloat((step % 4) + 1) / 4.0
        }
        withPerfSignpost("diff.apply")
        {
            showingModal = step.isMultiple(of: 2)
        }
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect)
    {
        let baseFrames = [
            CGRect(x: 32.0, y: 32.0, width: 92.0, height: 92.0),
            CGRect(x: 144.0, y: 32.0, width: 92.0, height: 92.0),
            CGRect(x: 32.0, y: 144.0, width: 92.0, height: 92.0),
            CGRect(x: 144.0, y: 144.0, width: 92.0, height: 92.0),
        ]
        let offsets = [
            CGPoint(x: -28.0, y: -18.0),
            CGPoint(x: 32.0, y: -12.0),
            CGPoint(x: -20.0, y: 26.0),
            CGPoint(x: 26.0, y: 22.0),
        ]
        let colors: [UIColor] = [
            UIColor(red: 0.90, green: 0.30, blue: 0.30, alpha: 1.0),
            UIColor(red: 0.30, green: 0.90, blue: 0.30, alpha: 1.0),
            UIColor(red: 0.30, green: 0.30, blue: 0.90, alpha: 1.0),
            UIColor(red: 0.90, green: 0.90, blue: 0.30, alpha: 1.0),
        ]
        for index in 0..<baseFrames.count
        {
            let base = baseFrames[index]
            let delta = offsets[index]
            let cardRect = base.offsetBy(dx: delta.x * phase, dy: delta.y * phase)
            colors[index].setFill()
            UIBezierPath(roundedRect: cardRect, cornerRadius: 16.0).fill()
        }

        if showingModal
        {
            UIColor.black.withAlphaComponent(0.35).setFill()
            UIRectFill(bounds)

            let modalRect = CGRect(x: bounds.midX - 120.0, y: bounds.midY - 70.0, width: 240.0, height: 140.0)
            UIColor.white.setFill()
            UIBezierPath(roundedRect: modalRect, cornerRadius: 18.0).fill()
            let attrs: [NSAttributedString.Key: Any] = [
                .font: UIFont.systemFont(ofSize: 18.0, weight: .bold),
                .foregroundColor: UIColor(red: 0.16, green: 0.20, blue: 0.28, alpha: 1.0),
            ]
            ("Dismissable Modal" as NSString).draw(
                in: CGRect(x: modalRect.minX + 24.0, y: modalRect.midY - 12.0, width: modalRect.width - 48.0, height: 24.0),
                withAttributes: attrs
            )
        }
    }
}

private final class LabelStackBenchView: UIView
{
    private var labels: [UILabel] = []

    override func layoutSubviews()
    {
        super.layoutSubviews()
        var y: CGFloat = 0.0
        for label in labels
        {
            label.frame = CGRect(x: 0.0, y: y, width: bounds.width, height: 18.0)
            y += 20.0
        }
    }

    func install(lines: [String])
    {
        clear()
        labels.reserveCapacity(lines.count)
        for line in lines
        {
            let label = UILabel(frame: .zero)
            label.text = line
            label.textColor = UIColor(red: 0.12, green: 0.14, blue: 0.18, alpha: 1.0)
            label.font = UIFont.systemFont(ofSize: 14.0, weight: .regular)
            addSubview(label)
            labels.append(label)
        }
        setNeedsLayout()
        layoutIfNeeded()
    }

    private func clear()
    {
        labels.forEach { $0.removeFromSuperview() }
        labels.removeAll(keepingCapacity: true)
    }
}

private final class ControlSetBenchView: UIView
{
    private let titleLabel = UILabel()
    private let detailLabel = UILabel()
    private let actionButton = UIButton(type: .system)
    private let progressView = ProgressBarBenchView(frame: .zero)
    private let spinnerView = SpinnerBenchView(frame: .zero)
    private let toggleView = ToggleBenchView(frame: .zero)
    private let slider = UISlider(frame: .zero)
    private let previewImageView: UIImageView

    init(frame: CGRect, image: UIImage)
    {
        self.previewImageView = UIImageView(image: image)
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.96, green: 0.97, blue: 0.99, alpha: 1.0)
        layer.cornerRadius = 14.0
        clipsToBounds = true

        titleLabel.font = .systemFont(ofSize: 18.0, weight: .semibold)
        titleLabel.text = "Controls Showcase"

        detailLabel.font = .systemFont(ofSize: 12.0, weight: .medium)
        detailLabel.numberOfLines = 2

        actionButton.configuration = .filled()

        slider.minimumValue = 0.0
        slider.maximumValue = 1.0

        previewImageView.clipsToBounds = true
        previewImageView.contentMode = .scaleAspectFill
        previewImageView.layer.cornerRadius = 12.0
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        titleLabel.frame = CGRect(x: 18, y: 14, width: bounds.width - 36, height: 24)
        detailLabel.frame = CGRect(x: 18, y: 38, width: bounds.width - 36, height: 32)
        progressView.frame = CGRect(x: 18, y: 82, width: bounds.width - 36, height: 12)
        spinnerView.frame = CGRect(x: 18, y: 108, width: 24, height: 24)
        actionButton.frame = CGRect(x: 56, y: 100, width: 136, height: 40)
        toggleView.frame = CGRect(x: 18, y: 152, width: 60, height: 28)
        slider.frame = CGRect(x: 94, y: 152, width: bounds.width - 188, height: 28)
        previewImageView.frame = CGRect(x: bounds.width - 86, y: 100, width: 68, height: 80)
    }

    func installDeck(palettePhase: Int)
    {
        removeAllControls()
        deckViews().forEach(addSubview)
        mutate(statePhase: palettePhase)
        setNeedsLayout()
        layoutIfNeeded()
    }

    func mutate(statePhase: Int)
    {
        let accent = Self.accentColor(phase: statePhase)
        withPerfSignpost("diff.apply")
        {
            backgroundColor = accent.withAlphaComponent(0.10)
            titleLabel.textColor = UIColor(red: 0.10, green: 0.12, blue: 0.18, alpha: 1.0)
            detailLabel.textColor = accent
            detailLabel.text = "State \(statePhase % 9) • Slider \(statePhase % 10)"

            var config = actionButton.configuration ?? .filled()
            config.title = statePhase.isMultiple(of: 2) ? "Press Me" : "Confirm"
            config.baseBackgroundColor = accent
            config.baseForegroundColor = .white
            actionButton.configuration = config

            progressView.progress = 0.18 + CGFloat(statePhase % 7) * 0.11
            progressView.phase = CGFloat(statePhase % 16) / 16.0
            spinnerView.phase = CGFloat((statePhase * 3) % 32) / 32.0
            toggleView.phase = statePhase.isMultiple(of: 2) ? 1.0 : 0.0
            slider.value = Float(statePhase % 10) / 9.0
            previewImageView.alpha = 0.72 + CGFloat(statePhase % 4) * 0.07
        }
    }

    func runButtonPressResponse(step: Int)
    {
        let accent = Self.accentColor(phase: step)
        withPerfSignpost("diff.apply")
        {
            var config = actionButton.configuration ?? .filled()
            config.baseBackgroundColor = accent
            config.baseForegroundColor = .white
            config.title = step.isMultiple(of: 2) ? "Press Me" : "Confirm"
            actionButton.configuration = config
            detailLabel.text = step.isMultiple(of: 2)
                ? "Pressed state armed."
                : "Released state armed."
        }
        withPerfSignpost("first.interactive")
        {
            actionButton.transform = step.isMultiple(of: 2)
                ? CGAffineTransform(scaleX: 0.96, y: 0.96)
                : .identity
            previewImageView.alpha = step.isMultiple(of: 2) ? 0.84 : 1.0
        }
    }

    func runSliderScrubResponse(step: Int)
    {
        let value = Float(step % 11) / 10.0
        withPerfSignpost("diff.apply")
        {
            slider.value = value
            progressView.progress = 0.12 + CGFloat(value) * 0.76
            detailLabel.text = String(format: "Slider %.0f%%", value * 100.0)
        }
        withPerfSignpost("first.interactive")
        {
            previewImageView.alpha = 0.70 + CGFloat(value) * 0.30
        }
    }

    func removeAllControls()
    {
        deckViews().forEach { $0.removeFromSuperview() }
    }

    private func deckViews() -> [UIView]
    {
        [
            titleLabel,
            detailLabel,
            progressView,
            spinnerView,
            actionButton,
            toggleView,
            slider,
            previewImageView,
        ]
    }

    private static func accentColor(phase: Int) -> UIColor
    {
        switch phase % 5
        {
        case 0:
            return UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0)
        case 1:
            return UIColor(red: 0.96, green: 0.38, blue: 0.24, alpha: 1.0)
        case 2:
            return UIColor(red: 0.22, green: 0.72, blue: 0.42, alpha: 1.0)
        case 3:
            return UIColor(red: 0.58, green: 0.38, blue: 0.96, alpha: 1.0)
        default:
            return UIColor(red: 0.16, green: 0.68, blue: 0.86, alpha: 1.0)
        }
    }
}

private final class FormJourneyBenchView: UIView, UIPickerViewDataSource, UIPickerViewDelegate
{
    private let usernameField = UITextField()
    private let passwordField = UITextField()
    private let actionButton = UIButton(type: .system)
    private let rolePicker = UIPickerView()
    private let statusLabel = UILabel()
    private let roles = [
        "Conference Pass",
        "Guest Access",
        "Team Member",
        "Partner",
        "Moderator",
    ]

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.95, green: 0.97, blue: 1.0, alpha: 1.0)

        usernameField.borderStyle = .roundedRect
        usernameField.placeholder = "Display name"
        usernameField.autocorrectionType = .no

        passwordField.borderStyle = .roundedRect
        passwordField.placeholder = "Password"
        passwordField.isSecureTextEntry = true
        passwordField.autocorrectionType = .no

        actionButton.configuration = .filled()
        actionButton.setTitle("Create Mission", for: .normal)

        rolePicker.dataSource = self
        rolePicker.delegate = self

        statusLabel.font = .systemFont(ofSize: 13.0)
        statusLabel.textColor = UIColor(white: 0.28, alpha: 1.0)
        statusLabel.numberOfLines = 2

        addSubview(usernameField)
        addSubview(passwordField)
        addSubview(actionButton)
        addSubview(rolePicker)
        addSubview(statusLabel)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        usernameField.frame = CGRect(x: 18, y: 32, width: 250, height: 44)
        passwordField.frame = CGRect(x: 18, y: 92, width: 250, height: 44)
        actionButton.frame = CGRect(x: 18, y: 152, width: 190, height: 44)
        rolePicker.frame = CGRect(x: bounds.width - 230, y: 24, width: 212, height: 180)
        statusLabel.frame = CGRect(x: 18, y: 214, width: bounds.width - 36, height: 42)
    }

    func runJourney(step: Int)
    {
        usernameField.text = "Pilot \(step % 7)"
        passwordField.text = "Orbit12\(step % 10)"
        let roleIndex = step % roles.count
        rolePicker.selectRow(roleIndex, inComponent: 0, animated: false)
        actionButton.transform = CGAffineTransform(scaleX: 0.985, y: 0.985)
        statusLabel.text = "Welcome Pilot \(step % 7)! Assigned role: \(roles[roleIndex])"
    }

    func numberOfComponents(in pickerView: UIPickerView) -> Int
    {
        1
    }

    func pickerView(_ pickerView: UIPickerView, numberOfRowsInComponent component: Int) -> Int
    {
        roles.count
    }

    func pickerView(
        _ pickerView: UIPickerView,
        titleForRow row: Int,
        forComponent component: Int
    ) -> String?
    {
        roles[row]
    }
}

private final class AuthoringTextFieldsBenchView: UIView
{
    private let usernameField = UITextField()
    private let bioView = UITextView()
    private let passwordField = UITextField()
    private let statusLabel = UILabel()

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.97, green: 0.98, blue: 1.0, alpha: 1.0)

        usernameField.borderStyle = .roundedRect
        usernameField.autocorrectionType = .no
        usernameField.autocapitalizationType = .none
        usernameField.placeholder = "username"

        bioView.layer.cornerRadius = 12.0
        bioView.layer.borderWidth = 1.0
        bioView.layer.borderColor = UIColor(red: 0.82, green: 0.86, blue: 0.93, alpha: 1.0).cgColor
        bioView.backgroundColor = .white
        bioView.font = .systemFont(ofSize: 14.0)
        bioView.textContainerInset = UIEdgeInsets(top: 10, left: 8, bottom: 10, right: 8)

        passwordField.borderStyle = .roundedRect
        passwordField.autocorrectionType = .no
        passwordField.autocapitalizationType = .none
        passwordField.placeholder = "password"
        passwordField.isSecureTextEntry = true

        statusLabel.font = .systemFont(ofSize: 13.0, weight: .medium)
        statusLabel.numberOfLines = 2
        statusLabel.textColor = UIColor(red: 0.28, green: 0.32, blue: 0.40, alpha: 1.0)

        addSubview(usernameField)
        addSubview(bioView)
        addSubview(passwordField)
        addSubview(statusLabel)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        usernameField.frame = CGRect(x: 18, y: 20, width: bounds.width - 36, height: 42)
        bioView.frame = CGRect(x: 18, y: 76, width: bounds.width - 36, height: 112)
        passwordField.frame = CGRect(x: 18, y: 202, width: bounds.width - 36, height: 42)
        statusLabel.frame = CGRect(x: 18, y: 256, width: bounds.width - 36, height: 40)
    }

    func runEditCycle(step: Int)
    {
        withPerfSignpost("diff.apply")
        {
            usernameField.text = "pilot_\(step % 19).one"
            bioView.text = "Orbit clearance \(step % 7). Preparing dock \(step % 5) with status \(step % 11)."
            passwordField.isSecureTextEntry = false
            passwordField.text = "secret\(42 + step % 31)"
            passwordField.isSecureTextEntry = true
            statusLabel.text = step.isMultiple(of: 2)
                ? "Validation clear. Secure remask applied."
                : "Checking normalization and caret update."
        }
        withPerfSignpost("text.measure")
        {
            _ = bioView.sizeThatFits(
                CGSize(width: bounds.width - 36, height: CGFloat.greatestFiniteMagnitude)
            )
        }
        withPerfSignpost("first.interactive")
        {
            statusLabel.alpha = step.isMultiple(of: 2) ? 1.0 : 0.88
        }
    }

    func runFocusCycle(step: Int)
    {
        let focusColor = UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0)
        let idleColor = UIColor(red: 0.82, green: 0.86, blue: 0.93, alpha: 1.0)
        withPerfSignpost("diff.apply")
        {
            statusLabel.text = step.isMultiple(of: 2)
                ? "Preparing focus handoff."
                : "Preparing responder update."
        }
        withPerfSignpost("first.interactive")
        {
            switch step % 3
            {
            case 0:
                _ = usernameField.becomeFirstResponder()
                usernameField.backgroundColor = focusColor.withAlphaComponent(0.12)
                bioView.layer.borderColor = idleColor.cgColor
                passwordField.backgroundColor = .white
            case 1:
                _ = bioView.becomeFirstResponder()
                usernameField.backgroundColor = .white
                bioView.layer.borderColor = focusColor.cgColor
                passwordField.backgroundColor = .white
            default:
                _ = passwordField.becomeFirstResponder()
                usernameField.backgroundColor = .white
                bioView.layer.borderColor = idleColor.cgColor
                passwordField.backgroundColor = focusColor.withAlphaComponent(0.12)
            }
            statusLabel.alpha = step.isMultiple(of: 2) ? 1.0 : 0.86
        }
    }
}

private final class PopupWheelPickerBenchView: UIView, UIPickerViewDataSource, UIPickerViewDelegate
{
    private let panelView = UIView()
    private let titleLabel = UILabel()
    private let stateLabel = UILabel()
    private let picker = UIPickerView()
    private let options = [
        "Explorer",
        "Navigator",
        "Commander",
        "Systems",
        "Guest",
        "Dock",
        "Control",
    ]
    private var panelOpen = true

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.95, green: 0.97, blue: 1.0, alpha: 1.0)

        panelView.backgroundColor = .white
        panelView.layer.cornerRadius = 18.0
        panelView.layer.shadowColor = UIColor.black.cgColor
        panelView.layer.shadowOpacity = 0.10
        panelView.layer.shadowRadius = 16.0
        panelView.layer.shadowOffset = CGSize(width: 0, height: 10)

        titleLabel.font = .boldSystemFont(ofSize: 17.0)
        titleLabel.text = "Crew Role"

        stateLabel.font = .systemFont(ofSize: 13.0)
        stateLabel.textColor = UIColor(red: 0.30, green: 0.34, blue: 0.42, alpha: 1.0)

        picker.dataSource = self
        picker.delegate = self

        panelView.addSubview(titleLabel)
        panelView.addSubview(stateLabel)
        panelView.addSubview(picker)
        addSubview(panelView)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        let panelWidth = min(bounds.width - 32, 240.0)
        let panelHeight: CGFloat = panelOpen ? 212.0 : 72.0
        panelView.frame = CGRect(
            x: bounds.midX - panelWidth * 0.5,
            y: bounds.midY - panelHeight * 0.5,
            width: panelWidth,
            height: panelHeight
        )
        titleLabel.frame = CGRect(x: 16, y: 14, width: panelWidth - 32, height: 22)
        stateLabel.frame = CGRect(x: 16, y: 40, width: panelWidth - 32, height: 18)
        picker.frame = CGRect(x: 8, y: 64, width: panelWidth - 16, height: 132)
        picker.alpha = panelOpen ? 1.0 : 0.0
        picker.isHidden = !panelOpen
    }

    func runInteraction(step: Int)
    {
        withPerfSignpost("diff.apply")
        {
            panelOpen = true
            let selection = step % options.count
            picker.selectRow(selection, inComponent: 0, animated: false)
            stateLabel.text = "Selected \(options[selection])"
            panelView.backgroundColor = step.isMultiple(of: 2)
                ? UIColor.white
                : UIColor(red: 0.96, green: 0.98, blue: 1.0, alpha: 1.0)
        }
        withPerfSignpost("first.interactive")
        {
            panelOpen = !step.isMultiple(of: 3)
        }
    }

    func numberOfComponents(in pickerView: UIPickerView) -> Int
    {
        1
    }

    func pickerView(_ pickerView: UIPickerView, numberOfRowsInComponent component: Int) -> Int
    {
        options.count
    }

    func pickerView(
        _ pickerView: UIPickerView,
        titleForRow row: Int,
        forComponent component: Int
    ) -> String?
    {
        options[row]
    }
}

private final class BurstEmitterBenchView: UIView
{
    private let emitterLayer = CAEmitterLayer()
    private let statusLabel = UILabel()

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.08, green: 0.10, blue: 0.16, alpha: 1.0)

        emitterLayer.emitterShape = .sphere
        emitterLayer.emitterMode = .surface
        emitterLayer.renderMode = .unordered
        emitterLayer.birthRate = 1.0
        emitterLayer.emitterCells = [Self.makeEmitterCell()]
        layer.addSublayer(emitterLayer)

        statusLabel.font = .systemFont(ofSize: 13.0, weight: .medium)
        statusLabel.textColor = UIColor.white.withAlphaComponent(0.92)
        addSubview(statusLabel)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        emitterLayer.frame = bounds
        emitterLayer.emitterPosition = CGPoint(x: bounds.midX, y: bounds.midY)
        statusLabel.frame = CGRect(x: 16, y: bounds.height - 34, width: bounds.width - 32, height: 18)
    }

    func runSample(step: Int)
    {
        let phase = CGFloat(step % 9) / 8.0
        withPerfSignpost("diff.apply")
        {
            emitterLayer.emitterPosition = CGPoint(
                x: bounds.width * (0.25 + phase * 0.5),
                y: bounds.height * (0.35 + phase * 0.2)
            )
            emitterLayer.emitterSize = CGSize(width: 24 + phase * 20, height: 24 + phase * 20)
            emitterLayer.birthRate = Float(1.0 + phase * 0.5)
            if let cell = emitterLayer.emitterCells?.first?.copy() as? CAEmitterCell
            {
                cell.birthRate = 25.0
                cell.lifetime = 1.0
                cell.velocity = CGFloat(300.0 + phase * 40.0)
                cell.scale = CGFloat(0.10 + phase * 0.03)
                cell.emissionRange = CGFloat.pi * 2.0
                emitterLayer.emitterCells = [cell]
            }
            statusLabel.text = "Emitter phase \(step % 9)"
        }
    }

    private static func makeEmitterCell() -> CAEmitterCell
    {
        let cell = CAEmitterCell()
        cell.contents = makeParticleImage()
        cell.birthRate = 25.0
        cell.lifetime = 1.0
        cell.velocity = 300.0
        cell.scale = 0.10
        cell.alphaSpeed = -0.8
        cell.emissionRange = CGFloat.pi * 2.0
        return cell
    }

    private static func makeParticleImage() -> CGImage?
    {
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: 24, height: 24))
        let image = renderer.image
        {
            ctx in
            let rect = CGRect(x: 4, y: 4, width: 16, height: 16)
            UIColor(red: 0.98, green: 0.94, blue: 0.56, alpha: 1.0).setFill()
            ctx.cgContext.fillEllipse(in: rect)
        }
        return image.cgImage
    }
}

private final class SurfaceRouterComposeBenchView: UIView
{
    private let baseSurface = UIView()
    private let nextSurface = UIView()
    private let baseCard = UIView()
    private let nextCard = UIView()
    private let overlayView = UIView()
    private let popupView = UIView()
    private let popupLabel = UILabel()

    private var phase: CGFloat = 0.0
    private var showingNext = false
    private var showingOverlay = false
    private var showingPopup = false

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.96, green: 0.97, blue: 0.99, alpha: 1.0)

        baseSurface.backgroundColor = UIColor(red: 0.12, green: 0.16, blue: 0.24, alpha: 1.0)
        nextSurface.backgroundColor = UIColor(red: 0.22, green: 0.28, blue: 0.40, alpha: 1.0)
        baseCard.backgroundColor = UIColor(red: 0.32, green: 0.66, blue: 0.94, alpha: 1.0)
        nextCard.backgroundColor = UIColor(red: 0.92, green: 0.40, blue: 0.46, alpha: 1.0)

        [baseSurface, nextSurface, baseCard, nextCard, overlayView, popupView].forEach
        {
            view in
            view.layer.cornerRadius = 18.0
        }

        baseSurface.addSubview(baseCard)
        nextSurface.addSubview(nextCard)
        addSubview(baseSurface)
        addSubview(nextSurface)

        overlayView.backgroundColor = UIColor.black.withAlphaComponent(0.34)
        addSubview(overlayView)

        popupView.backgroundColor = UIColor(red: 0.24, green: 0.60, blue: 0.92, alpha: 0.96)
        popupLabel.font = .boldSystemFont(ofSize: 16.0)
        popupLabel.textAlignment = .center
        popupLabel.textColor = .white
        popupLabel.text = "Overlay Compose"
        popupView.addSubview(popupLabel)
        addSubview(popupView)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        let surfaceFrame = CGRect(x: 18, y: 18, width: bounds.width - 36, height: bounds.height - 36)
        baseSurface.frame = surfaceFrame.offsetBy(dx: showingNext ? -24 * phase : 0, dy: -18 * phase)
        nextSurface.frame = surfaceFrame.offsetBy(dx: showingNext ? 24 * (1 - phase) : 24, dy: 18 * (1 - phase))
        nextSurface.alpha = showingNext ? 1.0 : 0.0

        baseCard.frame = CGRect(x: 28, y: 28, width: baseSurface.bounds.width * 0.48, height: baseSurface.bounds.height * 0.48)
        nextCard.frame = CGRect(x: 28, y: 28, width: nextSurface.bounds.width * 0.48, height: nextSurface.bounds.height * 0.48)

        overlayView.frame = bounds
        overlayView.alpha = showingOverlay ? 1.0 : 0.0
        overlayView.isHidden = !showingOverlay

        popupView.frame = CGRect(
            x: bounds.midX - 110,
            y: bounds.midY - 56,
            width: 220,
            height: 112
        )
        popupLabel.frame = popupView.bounds
        popupView.alpha = showingPopup ? 1.0 : 0.0
        popupView.isHidden = !showingPopup
    }

    func runComposition(step: Int)
    {
        withPerfSignpost("transition")
        {
            phase = CGFloat((step % 5) + 1) / 5.0
            showingNext = !step.isMultiple(of: 2)
        }
        withPerfSignpost("diff.apply")
        {
            showingOverlay = !step.isMultiple(of: 3)
            showingPopup = step.isMultiple(of: 2)
            popupLabel.text = showingPopup ? "Overlay Compose \(step % 7)" : "Overlay Compose"
        }
    }
}

private final class OrchestrationBenchView: UIView
{
    private let cardViews: [UIView] = (0..<4).map { _ in UIView() }
    private let overlayView = UIView()
    private let modalView = UIView()
    private let modalLabel = UILabel()

    var phase: CGFloat = 0.0
    var showingModal: Bool = false

    override init(frame: CGRect)
    {
        super.init(frame: frame)
        backgroundColor = UIColor(white: 0.97, alpha: 1.0)

        let colors: [UIColor] = [
            UIColor(red: 0.90, green: 0.30, blue: 0.30, alpha: 1.0),
            UIColor(red: 0.30, green: 0.90, blue: 0.30, alpha: 1.0),
            UIColor(red: 0.30, green: 0.30, blue: 0.90, alpha: 1.0),
            UIColor(red: 0.90, green: 0.90, blue: 0.30, alpha: 1.0),
        ]
        for (index, view) in cardViews.enumerated()
        {
            view.backgroundColor = colors[index]
            view.layer.cornerRadius = 16.0
            addSubview(view)
        }

        overlayView.backgroundColor = UIColor.black.withAlphaComponent(0.35)
        addSubview(overlayView)

        modalView.backgroundColor = UIColor.white
        modalView.layer.cornerRadius = 18.0
        modalView.layer.shadowColor = UIColor.black.cgColor
        modalView.layer.shadowOpacity = 0.12
        modalView.layer.shadowRadius = 18.0
        modalView.layer.shadowOffset = CGSize(width: 0, height: 10)

        modalLabel.text = "Dismissable Modal"
        modalLabel.font = .boldSystemFont(ofSize: 18.0)
        modalLabel.textAlignment = .center
        modalView.addSubview(modalLabel)
        addSubview(modalView)
    }

    required init?(coder: NSCoder)
    {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews()
    {
        super.layoutSubviews()
        let baseFrames = [
            CGRect(x: 32, y: 32, width: 92, height: 92),
            CGRect(x: 144, y: 32, width: 92, height: 92),
            CGRect(x: 32, y: 144, width: 92, height: 92),
            CGRect(x: 144, y: 144, width: 92, height: 92),
        ]
        let offsets = [
            CGPoint(x: -28, y: -18),
            CGPoint(x: 32, y: -12),
            CGPoint(x: -20, y: 26),
            CGPoint(x: 26, y: 22),
        ]
        for (index, card) in cardViews.enumerated()
        {
            let base = baseFrames[index]
            let delta = offsets[index]
            card.frame = base.offsetBy(dx: delta.x * phase, dy: delta.y * phase)
        }

        overlayView.frame = bounds
        overlayView.alpha = showingModal ? 1.0 : 0.0
        overlayView.isHidden = !showingModal

        modalView.frame = CGRect(
            x: bounds.midX - 120,
            y: bounds.midY - 70,
            width: 240,
            height: 140
        )
        modalLabel.frame = CGRect(x: 18, y: 46, width: modalView.bounds.width - 36, height: 30)
        modalView.alpha = showingModal ? 1.0 : 0.0
        modalView.isHidden = !showingModal
    }
}

@MainActor
private final class OxideUIKitBenchmarkAssets
{
    static let shared = OxideUIKitBenchmarkAssets()

    let checkerImage: UIImage
    let checkerPNGData: Data
    let nineSliceImage: UIImage

    private init()
    {
        self.checkerImage = Self.makeCheckerImage(size: CGSize(width: 128, height: 128))
        self.checkerPNGData = checkerImage.pngData() ?? Data()
        self.nineSliceImage = Self.makeNineSliceImage()
    }

    private static func makeCheckerImage(size: CGSize) -> UIImage
    {
        let renderer = UIGraphicsImageRenderer(size: size)
        return renderer.image
        {
            ctx in
            let cell: CGFloat = 16.0
            for y in stride(from: CGFloat.zero, to: size.height, by: cell)
            {
                for x in stride(from: CGFloat.zero, to: size.width, by: cell)
                {
                    let parity = Int((x / cell) + (y / cell)) % 2
                    let white = parity == 0 ? CGFloat(0.86) : CGFloat(0.70)
                    UIColor(white: white, alpha: 1.0).setFill()
                    ctx.fill(CGRect(x: x, y: y, width: cell, height: cell))
                }
            }
        }
    }

    private static func makeNineSliceImage() -> UIImage
    {
        let size = CGSize(width: 32, height: 32)
        let renderer = UIGraphicsImageRenderer(size: size)
        let base = renderer.image
        {
            ctx in
            UIColor(red: 0.20, green: 0.55, blue: 1.0, alpha: 1.0).setFill()
            ctx.fill(CGRect(origin: .zero, size: size))
            UIColor.white.setStroke()
            let path = UIBezierPath(
                roundedRect: CGRect(x: 2, y: 2, width: 28, height: 28),
                cornerRadius: 8
            )
            path.lineWidth = 2.0
            path.stroke()
        }
        return base.resizableImage(withCapInsets: UIEdgeInsets(top: 12, left: 12, bottom: 12, right: 12))
    }
}

@MainActor
private func decodedCheckerImage(from data: Data) -> UIImage?
{
    guard let image = UIImage(data: data, scale: 1.0) else
    {
        return nil
    }
    let renderer = UIGraphicsImageRenderer(size: image.size)
    return renderer.image
    {
        _ in
        image.draw(at: .zero)
    }
}

@MainActor
private func optimizedDecodedCheckerImage(from data: Data) -> UIImage?
{
    let options: CFDictionary = [
        kCGImageSourceShouldCache: true,
        kCGImageSourceShouldCacheImmediately: true,
    ] as CFDictionary
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
        let cgImage = CGImageSourceCreateImageAtIndex(source, 0, options)
    else
    {
        return nil
    }
    return UIImage(cgImage: cgImage, scale: 1.0, orientation: .up)
}

@MainActor
private func flatRectLifecycleIterations(count: Int) -> Int
{
    switch count
    {
    case 0...10:
        return 24
    case 11...100:
        return 12
    default:
        return 6
    }
}

@MainActor
private func makeEmptyRootMountBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 96)
    {
        let view = UIView(frame: .zero)
        view.backgroundColor = .clear
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeFlatRectMountBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        let view = FlatRectGridBenchView(frame: .zero)
        view.install(count: count, palettePhase: 0)
        host.mount(view, size: CGSize(width: 420, height: 760))
    }
}

@MainActor
private func makeFlatRectMutateBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = FlatRectGridBenchView(frame: .zero)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: CGSize(width: 420, height: 760))
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        palettePhase += 1
        view.mutate(palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func makeFlatRectRemoveAllBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = FlatRectGridBenchView(frame: .zero)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        view.removeAllRects()
        host.commit(view)
        view.install(count: count, palettePhase: 0)
    }
}

@MainActor
private func makeFlatRectRemountBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = FlatRectGridBenchView(frame: .zero)
    host.mount(view, size: primitiveLifecycleViewport())
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        view.install(count: count, palettePhase: 0)
        host.commit(view)
        view.removeAllRects()
    }
}

@MainActor
private func makeOptimizedFlatRectMountBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        let view = OptimizedFlatRectGridBenchView(frame: .zero)
        view.install(count: count, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeOptimizedFlatRectMutateBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedFlatRectGridBenchView(frame: .zero)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        palettePhase += 1
        view.mutate(palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedLabelMountBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        let view = OptimizedLabelGridBenchView(frame: .zero)
        view.install(count: count, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeOptimizedLabelMutateBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedLabelGridBenchView(frame: .zero)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        palettePhase += 1
        view.mutate(palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedCardMountBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        let view = OptimizedCardGridBenchView(frame: .zero)
        view.install(count: count, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeOptimizedCardMutateBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedCardGridBenchView(frame: .zero)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        palettePhase += 1
        view.mutate(palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedImageMountBenchmark(
    testName: String,
    count: Int,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        let view = OptimizedImageGridBenchView(frame: .zero, image: image)
        view.install(count: count, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeOptimizedImageMutateBenchmark(
    testName: String,
    count: Int,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedImageGridBenchView(frame: .zero, image: image)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        palettePhase += 1
        view.mutate(palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func primitiveLifecycleViewport() -> CGSize
{
    CGSize(width: 420, height: 760)
}

@MainActor
private func makeLabelMountBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        let view = LabelGridBenchView(frame: .zero)
        view.install(count: count, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeLabelMutateBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = LabelGridBenchView(frame: .zero)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        palettePhase += 1
        view.mutate(palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func makeCardMountBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        let view = CardGridBenchView(frame: .zero)
        view.install(count: count, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeCardMutateBenchmark(
    testName: String,
    count: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = CardGridBenchView(frame: .zero)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        palettePhase += 1
        view.mutate(palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func makeImageMountBenchmark(
    testName: String,
    count: Int,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        let view = ImageGridBenchView(frame: .zero, image: image)
        view.install(count: count, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeImageMutateBenchmark(
    testName: String,
    count: Int,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = ImageGridBenchView(frame: .zero, image: image)
    view.install(count: count, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: flatRectLifecycleIterations(count: count))
    {
        palettePhase += 1
        view.mutate(palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func makeControlSetMountBenchmark(
    testName: String,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 32)
    {
        let view = ControlSetBenchView(frame: .zero, image: image)
        view.installDeck(palettePhase: 0)
        host.mount(view, size: CGSize(width: 360, height: 220))
    }
}

@MainActor
private func makeControlSetMutateBenchmark(
    testName: String,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = ControlSetBenchView(frame: .zero, image: image)
    view.installDeck(palettePhase: 0)
    host.mount(view, size: CGSize(width: 360, height: 220))
    var statePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 32)
    {
        statePhase += 1
        view.mutate(statePhase: statePhase)
        host.commit(view)
    }
}

private func bridgeFileFixture(rowCount: Int) -> [String]
{
    (0..<rowCount).map
    {
        row in
        "ITEM-\(String(format: "%03d", row)) | Orbit \(row % 9) | Priority \(row % 3) | Owner \(row % 5)"
    }
}

private func bridgeJSONFixture(rowCount: Int) -> Data
{
    let rows = (0..<rowCount).map
    {
        row in
        [
            "title": "Feed \(row)",
            "accent": "\(row % 6)",
            "count": "\(40 + row)",
        ]
    }
    return (try? JSONSerialization.data(withJSONObject: rows, options: [])) ?? Data()
}

@MainActor
private func makeImageDecodeBenchmark(
    testName: String,
    pngData: Data
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 24)
    {
        _ = decodedCheckerImage(from: pngData)
    }
}

@MainActor
private func makeImageUploadBenchmark(
    testName: String,
    pngData: Data,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let decoded = decodedCheckerImage(from: pngData) ?? UIImage()
    let view = ImageGridBenchView(frame: .zero, image: decoded)
    host.mount(view, size: primitiveLifecycleViewport())
    return OxideUIKitBenchmark(testName: testName, iterations: 12)
    {
        view.install(count: 100, palettePhase: 0)
        host.commit(view)
    }
}

@MainActor
private func makeImageFirstVisibleBenchmark(
    testName: String,
    pngData: Data,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 12)
    {
        let decoded = decodedCheckerImage(from: pngData) ?? UIImage()
        let view = ImageGridBenchView(frame: .zero, image: decoded)
        view.install(count: 100, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeOptimizedImageDecodeBenchmark(
    testName: String,
    pngData: Data
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 24)
    {
        _ = optimizedDecodedCheckerImage(from: pngData)
    }
}

@MainActor
private func makeOptimizedImageUploadBenchmark(
    testName: String,
    pngData: Data,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let decoded = optimizedDecodedCheckerImage(from: pngData) ?? UIImage()
    let view = OptimizedImageGridBenchView(frame: .zero, image: decoded)
    host.mount(view, size: primitiveLifecycleViewport())
    return OxideUIKitBenchmark(testName: testName, iterations: 12)
    {
        view.install(count: 100, palettePhase: 0)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedImageFirstVisibleBenchmark(
    testName: String,
    pngData: Data,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 12)
    {
        let decoded = optimizedDecodedCheckerImage(from: pngData) ?? UIImage()
        let view = OptimizedImageGridBenchView(frame: .zero, image: decoded)
        view.install(count: 100, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeOptimizedLargeEditorKeystrokeBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedEditorBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 380, height: 460))
    var step = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 24)
    {
        step += 1
        view.runKeystrokeBurst(step: step)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedLargeEditorPasteBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedEditorBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 380, height: 460))
    return OxideUIKitBenchmark(testName: testName, iterations: 20)
    {
        view.runPaste()
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedLargeEditorSelectionReplaceBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedEditorBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 380, height: 460))
    return OxideUIKitBenchmark(testName: testName, iterations: 20)
    {
        view.runSelectionReplace()
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedInputFormJourneyBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedFormJourneyBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 560, height: 280))
    var step = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 24)
    {
        step += 1
        view.runJourney(step: step)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedOrchestrationJourneyBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedOrchestrationBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 300, height: 280))
    var step = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 20)
    {
        step += 1
        view.runJourney(step: step)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedCollectionNavigationBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedCollectionJourneyBenchView(frame: .zero, mode: .matrix)
    host.mount(view, size: CGSize(width: 360, height: 240))
    var anchor = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 18)
    {
        anchor = (anchor + 3) % 24
        withPerfSignpost("scroll")
        {
            view.select(item: anchor)
            host.commit(view)
            view.select(item: anchor + 3)
            host.commit(view)
            view.select(item: anchor + 6)
            host.commit(view)
            view.select(item: anchor + 2)
            host.commit(view)
        }
    }
}

@MainActor
private func makeOptimizedCollectionScrollBenchmark(
    testName: String,
    mode: CollectionBenchMode,
    phases: [CGFloat],
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedCollectionJourneyBenchView(frame: .zero, mode: mode)
    host.mount(view, size: CGSize(width: 360, height: 640))
    return OxideUIKitBenchmark(testName: testName, iterations: 12)
    {
        withPerfSignpost("scroll")
        {
            for phase in phases
            {
                view.scroll(to: phase)
                host.commit(view)
            }
        }
    }
}

@MainActor
private func makeOptimizedZoomImageGestureJourneyBenchmark(
    testName: String,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedImageTransformBenchView(frame: .zero, image: image)
    host.mount(view, size: CGSize(width: 280, height: 220))
    var step = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 24)
    {
        step += 1
        let scale = 1.0 + CGFloat(step % 6) * 0.12
        let tx = CGFloat((step % 5) - 2) * 12.0
        let ty = CGFloat((step % 4) - 2) * -9.0
        view.scale = scale
        view.offset = CGPoint(x: tx, y: ty)
        host.commit(view)
        view.scale = 1.0
        view.offset = .zero
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedPermissionCallbackBenchmark(testName: String) -> OxideUIKitBenchmark
{
    let callbackSum = UInt64Box()
    let callbacks: [(BenchPermissionState) -> Void] = [
        {
            state in
            callbackSum.value &+= state.status.rawValue &+ 1
        },
        {
            state in
            callbackSum.value &+= state.status.rawValue &+ 3
        },
        {
            state in
            callbackSum.value &+= state.status.rawValue &+ 5
        },
    ]
    var tick: UInt64 = 0
    var limited = false
    var status = BenchPermissionStatus.authorized
    return OxideUIKitBenchmark(testName: testName, iterations: 128)
    {
        withPerfSignpost("native.bridge")
        {
            tick &+= 17
            limited.toggle()
            status = limited ? .limited : .authorized
            let state = BenchPermissionState(status: status, timestampMs: tick)
            callbacks.forEach
            {
                callback in
                callback(state)
            }
            callbackSum.value &+= status.rawValue
        }
    }
}

@MainActor
private func makeOptimizedSensorLocationBridgeBenchmark(testName: String) -> OxideUIKitBenchmark
{
    var last: BenchLocationSample?
    var history = Array<BenchLocationSample?>(repeating: nil, count: 12)
    var historyCount = 0
    var nextIndex = 0
    var tick: UInt64 = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 96)
    {
        withPerfSignpost("native.bridge")
        {
            tick &+= 17
            let sample = BenchLocationSample(
                latitudeDeg: 37.7749 + Double(tick) * 0.000001,
                longitudeDeg: -122.4194 - Double(tick) * 0.000001,
                timestampMs: tick
            )
            last = sample
            history[nextIndex] = sample
            nextIndex = (nextIndex + 1) % history.count
            historyCount = min(historyCount + 1, history.count)
            _ = UInt64(historyCount) &+ (last?.timestampMs ?? 0)
        }
    }
}

@MainActor
private func makeOptimizedBluetoothCacheBridgeBenchmark(testName: String) -> OxideUIKitBenchmark
{
    var devices: [BenchBluetoothDevice] = []
    devices.reserveCapacity(24)
    var tick: UInt64 = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 96)
    {
        withPerfSignpost("native.bridge")
        {
            tick &+= 23
            let device = BenchBluetoothDevice(
                id: 10_000 + (tick % 12),
                lastSeenMs: tick,
                rssiDbm: -44
            )
            if let existing = devices.firstIndex(where: { $0.id == device.id })
            {
                devices[existing] = device
            }
            else if devices.count < 24
            {
                devices.append(device)
            }
            else if let oldest = devices.enumerated().min(by: { $0.element.lastSeenMs < $1.element.lastSeenMs })?.offset
            {
                devices[oldest] = device
            }
            _ = UInt64(devices.count) &+ 1
        }
    }
}

@MainActor
private func makeOptimizedPhotoImportThumbnailBenchmark(
    testName: String,
    pngData: Data,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 16)
    {
        let decoded = optimizedDecodedCheckerImage(from: pngData) ?? UIImage()
        let view = OptimizedImageGridBenchView(frame: .zero, image: decoded)
        view.install(count: 10, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeOptimizedFileImportRenderBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let lines = bridgeFileFixture(rowCount: 32)
    let view = OptimizedTextListBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 360, height: 720))
    var accentPhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 16)
    {
        accentPhase += 1
        view.install(lines: lines, accentPhase: accentPhase)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedSharePayloadPrepareBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let payload = [
        "1. Orbit telemetry card",
        "2. Damage report snapshot",
        "3. Field note export",
    ]
    let view = OptimizedTextListBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 320, height: 120))
    var accentPhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 24)
    {
        accentPhase += 1
        view.install(lines: payload, accentPhase: accentPhase)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedLocalJSONTransportRenderBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let payload = bridgeJSONFixture(rowCount: 48)
    let view = OptimizedTextListBenchView(frame: .zero)
    host.mount(view, size: primitiveLifecycleViewport())
    var accentPhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 12)
    {
        let rows =
            (try? JSONSerialization.jsonObject(with: payload, options: [])) as? [[String: String]] ?? []
        let lines = rows.map
        {
            row in
            "\(row["title"] ?? "Feed") | Accent \(row["accent"] ?? "0") | Count \(row["count"] ?? "0")"
        }
        accentPhase += 1
        view.install(lines: lines, accentPhase: accentPhase)
        host.commit(view)
    }
}

@MainActor
private func makeOptimizedLocalImageTransportRenderBenchmark(
    testName: String,
    pngData: Data,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 16)
    {
        let decoded = optimizedDecodedCheckerImage(from: pngData) ?? UIImage()
        let view = OptimizedImageGridBenchView(frame: .zero, image: decoded)
        view.install(count: 1, palettePhase: 0)
        host.mount(view, size: CGSize(width: 360, height: 280))
    }
}

@MainActor
private func makeOptimizedOpenCloseHeavyScreenBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let lines = bridgeFileFixture(rowCount: 32)
    return OxideUIKitBenchmark(testName: testName, iterations: 1)
    {
        for index in 0..<100
        {
            let view = OptimizedTextListBenchView(frame: .zero)
            view.install(lines: lines, accentPhase: index)
            host.mount(view, size: CGSize(width: 360, height: 640))
        }
    }
}

@MainActor
private func makeOptimizedTabSwitchHeavyBenchmark(
    testName: String,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let feedLines = bridgeFileFixture(rowCount: 32)
    return OxideUIKitBenchmark(testName: testName, iterations: 1)
    {
        for index in 0..<500
        {
            let view: UIView
            let size: CGSize
            switch index % 3
            {
            case 0:
                let feed = OptimizedTextListBenchView(frame: .zero)
                feed.install(lines: feedLines, accentPhase: index)
                view = feed
                size = CGSize(width: 360, height: 640)
            case 1:
                let grid = OptimizedImageGridBenchView(frame: .zero, image: image)
                grid.install(count: 120, palettePhase: index)
                view = grid
                size = CGSize(width: 360, height: 640)
            default:
                let orchestration = OptimizedOrchestrationBenchView(frame: .zero)
                orchestration.runJourney(step: index + 1)
                view = orchestration
                size = CGSize(width: 300, height: 280)
            }
            host.mount(view, size: size)
            host.commit(view)
        }
    }
}

@MainActor
private func makeOptimizedIdleAnimationBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = OptimizedTimelineBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 420, height: 220))
    return OxideUIKitBenchmark(testName: testName, iterations: 1)
    {
        for frame in 0..<600
        {
            view.phase = CGFloat(frame % 120) / 120.0
            host.commit(view)
        }
    }
}

@MainActor
private func makePhotoImportThumbnailBenchmark(
    testName: String,
    pngData: Data,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 16)
    {
        let decoded = decodedCheckerImage(from: pngData) ?? UIImage()
        let view = ImageGridBenchView(frame: .zero, image: decoded)
        view.install(count: 10, palettePhase: 0)
        host.mount(view, size: primitiveLifecycleViewport())
    }
}

@MainActor
private func makeFileImportRenderBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = LabelStackBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 360, height: 720))
    return OxideUIKitBenchmark(testName: testName, iterations: 16)
    {
        view.install(lines: bridgeFileFixture(rowCount: 32))
        host.commit(view)
    }
}

@MainActor
private func makeSharePayloadPrepareBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = LabelStackBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 320, height: 120))
    return OxideUIKitBenchmark(testName: testName, iterations: 24)
    {
        let payload = [
            "1. Orbit telemetry card",
            "2. Damage report snapshot",
            "3. Field note export",
        ].joined(separator: "\n")
        view.install(lines: payload.components(separatedBy: "\n"))
        host.commit(view)
    }
}

@MainActor
private func makeLocalJSONTransportRenderBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let payload = bridgeJSONFixture(rowCount: 48)
    let view = FlatRectGridBenchView(frame: .zero)
    host.mount(view, size: primitiveLifecycleViewport())
    return OxideUIKitBenchmark(testName: testName, iterations: 12)
    {
        let rows =
            (try? JSONSerialization.jsonObject(with: payload, options: [])) as? [[String: String]] ?? []
        view.install(count: rows.count, palettePhase: 1)
        host.commit(view)
    }
}

@MainActor
private func makeLocalImageTransportRenderBenchmark(
    testName: String,
    pngData: Data,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    OxideUIKitBenchmark(testName: testName, iterations: 16)
    {
        let decoded = decodedCheckerImage(from: pngData) ?? UIImage()
        let view = ImageGridBenchView(frame: .zero, image: decoded)
        view.install(count: 1, palettePhase: 0)
        host.mount(view, size: CGSize(width: 360, height: 280))
    }
}

@MainActor
private func makeButtonPressResponseBenchmark(
    testName: String,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = ControlSetBenchView(frame: .zero, image: image)
    view.installDeck(palettePhase: 0)
    host.mount(view, size: CGSize(width: 360, height: 220))
    var step = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 48)
    {
        step += 1
        view.runButtonPressResponse(step: step)
        host.commit(view)
    }
}

@MainActor
private func makeSliderScrubResponseBenchmark(
    testName: String,
    image: UIImage,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = ControlSetBenchView(frame: .zero, image: image)
    view.installDeck(palettePhase: 0)
    host.mount(view, size: CGSize(width: 360, height: 220))
    var step = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 48)
    {
        step += 1
        view.runSliderScrubResponse(step: step)
        host.commit(view)
    }
}

@MainActor
private func makeTextFocusResponseBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = AuthoringTextFieldsBenchView(frame: .zero)
    host.mount(view, size: CGSize(width: 320, height: 308))
    var step = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 24)
    {
        step += 1
        view.runFocusCycle(step: step)
        host.commit(view)
    }
}

@MainActor
private func makeReconcileMutationBenchmark(
    testName: String,
    dirtyNodes: Int,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = FlatRectGridBenchView(frame: .zero)
    view.install(count: 1_000, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var palettePhase = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 20)
    {
        palettePhase += 1
        view.mutateSubset(limit: dirtyNodes, palettePhase: palettePhase)
        host.commit(view)
    }
}

@MainActor
private func makeThemeSwapFullBenchmark(
    testName: String,
    host: PerfSurfaceHost
) -> OxideUIKitBenchmark
{
    let view = FlatRectGridBenchView(frame: .zero)
    view.install(count: 1_000, palettePhase: 0)
    host.mount(view, size: primitiveLifecycleViewport())
    var step = 0
    return OxideUIKitBenchmark(testName: testName, iterations: 20)
    {
        step += 1
        view.runThemeSwap(step: step)
        host.commit(view)
    }
}

@MainActor
private func makeOxideCameraPreviewBenchmark(
    testName: String,
    mode: OxideCameraRenderMode,
    source: OxideCameraTextureSource,
    host: PerfSurfaceHost,
    visibleTransport: OxideCameraVisiblePreviewTransport = .oxideRenderer
) -> OxideUIKitBenchmark?
{
    guard let harness = OxideCameraBenchmarkHarness(
        host: host,
        visibleTransport: visibleTransport
    ) else
    {
        return nil
    }
    guard harness.installAndWarm(mode: mode, source: source) else
    {
        return nil
    }
    let collectStageMetrics = cameraStageMeasurementEnabled()
    let opportunityCount = resolveCameraBenchmarkOpportunityCount(
        maximumFramesPerSecond: host.containerView.window?.windowScene?.screen.maximumFramesPerSecond
    )
    let opportunityIntervalSeconds = resolveCameraBenchmarkOpportunityIntervalSeconds(
        maximumFramesPerSecond: host.containerView.window?.windowScene?.screen.maximumFramesPerSecond
    )
    return OxideUIKitBenchmark(
        testName: testName,
        iterations: 1,
        signpostNames: oxideCameraBenchmarkSignpostNames(
            mode: mode,
            source: source,
            visibleTransport: visibleTransport
        ),
        prepareIteration: {
            guard harness.prepareForMeasuredPass() else
            {
                return false
            }
            if collectStageMetrics
            {
                harness.beginStageMeasurement()
            }
            return true
        },
        summaryLines: {
            var lines: [String] = []
            if let line = harness.contractSummaryLine()
            {
                lines.append(line)
            }
            guard collectStageMetrics else
            {
                return lines
            }
            if let line = harness.previewPlanSummaryLine()
            {
                lines.append(line)
            }
            if let line = harness.memorySummaryLine()
            {
                lines.append(line)
            }
            if let line = harness.stageSummaryLine()
            {
                lines.append(line)
            }
            harness.endStageMeasurement()
            return lines
        },
        tearDown: {
            harness.tearDown()
        }
    )
    {
        runPacedCameraPreviewWindow(
            opportunities: opportunityCount,
            opportunityIntervalSeconds: opportunityIntervalSeconds
        )
        {
            _ = harness.renderFrame()
        }
    }
}

@MainActor
private func makeOxideRealAppCameraPreviewBenchmark(
    testName: String,
    mode: OxideCameraRenderMode,
    source: OxideCameraTextureSource,
    host: PerfSurfaceHost,
    visibleTransport: OxideCameraVisiblePreviewTransport = .oxideRenderer
) -> OxideUIKitBenchmark?
{
    guard let harness = OxideRealAppCameraBenchmarkHarness(
        visibleTransport: visibleTransport
    ) else
    {
        return nil
    }
    guard harness.installAndWarm(mode: mode, source: source) else
    {
        return nil
    }
    let collectStageMetrics = cameraStageMeasurementEnabled()
    let opportunityCount = resolveCameraBenchmarkOpportunityCount(
        maximumFramesPerSecond: host.containerView.window?.windowScene?.screen.maximumFramesPerSecond
    )
    let opportunityIntervalSeconds = resolveCameraBenchmarkOpportunityIntervalSeconds(
        maximumFramesPerSecond: host.containerView.window?.windowScene?.screen.maximumFramesPerSecond
    )
    return OxideUIKitBenchmark(
        testName: testName,
        iterations: 1,
        signpostNames: oxideCameraBenchmarkSignpostNames(
            mode: mode,
            source: source,
            visibleTransport: visibleTransport
        ),
        prepareIteration: {
            guard harness.prepareForMeasuredPass() else
            {
                return false
            }
            if collectStageMetrics
            {
                harness.beginStageMeasurement()
            }
            return true
        },
        summaryLines: {
            var lines: [String] = []
            if let line = harness.contractSummaryLine()
            {
                lines.append(line)
            }
            guard collectStageMetrics else
            {
                return lines
            }
            if let line = harness.tickDebugSummaryLine()
            {
                lines.append(line)
            }
            if let line = harness.appHostDebugSummaryLine()
            {
                lines.append(line)
            }
            if let line = harness.previewPlanSummaryLine()
            {
                lines.append(line)
            }
            if let line = harness.memorySummaryLine()
            {
                lines.append(line)
            }
            if let line = harness.stageSummaryLine()
            {
                lines.append(line)
            }
            harness.endStageMeasurement()
            return lines
        },
        tearDown: {
            harness.tearDown()
        }
    )
    {
        runPacedCameraPreviewWindow(
            opportunities: opportunityCount,
            opportunityIntervalSeconds: opportunityIntervalSeconds
        )
        {
            harness.step()
        }
    }
}

@MainActor
private func makeAVFoundationPreviewBenchmark(
    testName: String,
    host: PerfSurfaceHost,
    includeVideoDataOutputSidecar: Bool = false
) -> OxideUIKitBenchmark?
{
    guard let harness = AVFoundationPreviewBenchmarkHarness(
        host: host,
        includeVideoDataOutputSidecar: includeVideoDataOutputSidecar
    ) else
    {
        return nil
    }
    guard harness.installAndWarm() else
    {
        return nil
    }
    let opportunityCount = resolveCameraBenchmarkOpportunityCount(
        maximumFramesPerSecond: host.containerView.window?.windowScene?.screen.maximumFramesPerSecond
    )
    let opportunityIntervalSeconds = resolveCameraBenchmarkOpportunityIntervalSeconds(
        maximumFramesPerSecond: host.containerView.window?.windowScene?.screen.maximumFramesPerSecond
    )
    return OxideUIKitBenchmark(
        testName: testName,
        iterations: 1,
        signpostNames: avFoundationPreviewBenchmarkSignpostNames,
        prepareIteration: {
            harness.prepareForMeasuredPass()
        },
        summaryLines: {
            if let line = harness.contractSummaryLine()
            {
                return [line]
            }
            return []
        },
        tearDown: {
            harness.tearDown()
        }
    )
    {
        runPacedCameraPreviewWindow(
            opportunities: opportunityCount,
            opportunityIntervalSeconds: opportunityIntervalSeconds,
            waitSignpostName: "baseline.preview.runloop"
        )
        {
            harness.step()
        }
    }
}

@MainActor
enum OxideUIKitBenchmarkCatalog
{
    static func makeBenchmark(named testName: String, host: PerfSurfaceHost) -> OxideUIKitBenchmark?
    {
        let normalizedTestName = testName.replacingOccurrences(of: "()", with: "")
        let assets = OxideUIKitBenchmarkAssets.shared

        switch normalizedTestName
        {
        case "testLabelEncode":
            let label = UILabel()
            label.numberOfLines = 0
            label.font = .systemFont(ofSize: 16.0)
            label.textColor = UIColor(white: 0.1, alpha: 1.0)
            host.mount(label, size: CGSize(width: 320, height: 80))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 64)
            {
                label.text = "Oxide perf audit label wrapping path for hot layout measurement."
                withPerfSignpost("text.measure")
                {
                    _ = label.sizeThatFits(CGSize(width: 320, height: 80))
                }
                host.commit(label)
            }
        case "testProgressBarEncode":
            let view = ProgressBarBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 260, height: 16))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                view.progress = 0.61
                view.phase = 0.0
                host.commit(view)
            }
        case "testSpinnerEncode":
            let view = SpinnerBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 32, height: 32))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                view.phase = 0.25
                host.commit(view)
            }
        case "testButtonEncode":
            let button = UIButton(type: .system)
            button.setTitle("Measure", for: .normal)
            button.configuration = .filled()
            host.mount(button, size: CGSize(width: 140, height: 40))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 64)
            {
                button.layoutIfNeeded()
                host.commit(button)
            }
        case "testToggleEncode":
            let view = ToggleBenchView(frame: .zero)
            view.phase = 1.0
            host.mount(view, size: CGSize(width: 48, height: 24))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                view.phase = 1.0
                host.commit(view)
            }
        case "testSliderEncode":
            let slider = UISlider(frame: .zero)
            slider.minimumValue = 0.0
            slider.maximumValue = 1.0
            slider.value = 0.68
            host.mount(slider, size: CGSize(width: 260, height: 32))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                slider.value = 0.68
                host.commit(slider)
            }
        case "testImageViewEncode":
            let imageView = UIImageView(image: assets.checkerImage)
            imageView.contentMode = .scaleAspectFit
            host.mount(imageView, size: CGSize(width: 220, height: 180))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                imageView.image = assets.checkerImage
                host.commit(imageView)
            }
        case "testNineSliceImageEncode":
            let imageView = UIImageView(image: assets.nineSliceImage)
            imageView.contentMode = .scaleToFill
            host.mount(imageView, size: CGSize(width: 240, height: 120))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                imageView.image = assets.nineSliceImage
                host.commit(imageView)
            }
        case "testCameraNV12OptimizedPreview":
            return makeOxideCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .nv12Optimized,
                source: .syntheticBenchmark,
                host: host
            )
        case "testCameraNV12LegacyPreview":
            return makeOxideCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .nv12Legacy,
                source: .syntheticBenchmark,
                host: host
            )
        case "testCameraBGRAPreview":
            return makeOxideCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .bgraBenchmark,
                source: .syntheticBenchmark,
                host: host
            )
        case "testCameraBGRALivePreview":
            return makeOxideCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .bgraBenchmark,
                source: .live,
                host: host
            )
        case "testCameraNV12OptimizedLivePreview":
            return makeOxideCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .nv12Optimized,
                source: .live,
                host: host
            )
        case "testCameraNV12LegacyLivePreview":
            return makeOxideCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .nv12Legacy,
                source: .live,
                host: host
            )
        case "testCameraNV12LegacyHybridPreviewLayerLivePreview":
            return makeOxideCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .nv12Legacy,
                source: .live,
                host: host,
                visibleTransport: .avFoundationPreviewLayer
            )
        case "testCameraNV12LegacyRealAppLivePreview":
            return makeOxideRealAppCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .nv12Legacy,
                source: .live,
                host: host
            )
        case "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview":
            return makeOxideRealAppCameraPreviewBenchmark(
                testName: normalizedTestName,
                mode: .nv12Legacy,
                source: .live,
                host: host,
                visibleTransport: .avFoundationPreviewLayer
            )
        case "testCameraAVFoundationPreviewLayerLivePreview":
            return makeAVFoundationPreviewBenchmark(
                testName: normalizedTestName,
                host: host
            )
        case "testCameraAVFoundationPreviewLayerSidecarLivePreview":
            return makeAVFoundationPreviewBenchmark(
                testName: normalizedTestName,
                host: host,
                includeVideoDataOutputSidecar: true
            )
        case "testCollectionViewEncode":
            let view = CollectionBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 360, height: 240))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                withPerfSignpost("scroll")
                {
                    view.scroll(to: 0.35)
                }
                host.commit(view)
            }
        case "testLayoutFlatGridRelayout":
            let view = FlatRectGridBenchView(frame: .zero)
            view.install(count: 240, palettePhase: 0)
            host.mount(view, size: CGSize(width: 360, height: 760))
            var landscape = false
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                landscape.toggle()
                view.frame.size = landscape
                    ? CGSize(width: 640, height: 420)
                    : CGSize(width: 360, height: 760)
                host.commit(view)
            }
        case "testLayoutDeepStackThemeSwap":
            let view = DeepStackBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 420, height: 820))
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                step += 1
                view.runThemeSwap(step: step)
                host.commit(view)
            }
        case "testLayoutGridSafeAreaSwap":
            let view = InsetGridBenchView(frame: .zero)
            view.gridView.install(count: 180, palettePhase: 0)
            host.mount(view, size: CGSize(width: 420, height: 760))
            var expanded = false
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                expanded.toggle()
                view.contentInsets = expanded
                    ? UIEdgeInsets(top: 44.0, left: 32.0, bottom: 28.0, right: 24.0)
                    : UIEdgeInsets(top: 8.0, left: 8.0, bottom: 8.0, right: 8.0)
                host.commit(view)
            }
        case "testLargeEditorKeystrokeBurst":
            let view = LargeEditorBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 380, height: 460))
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                step += 1
                view.runKeystrokeBurst(step: step)
                host.commit(view)
            }
        case "testLargeEditorPaste10KB":
            let view = LargeEditorBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 380, height: 460))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 20)
            {
                view.runPaste()
                host.commit(view)
            }
        case "testLargeEditorSelectionReplace":
            let view = LargeEditorBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 380, height: 460))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 20)
            {
                view.runSelectionReplace()
                host.commit(view)
            }
        case "testOptimizedLargeEditorKeystrokeBurst":
            return makeOptimizedLargeEditorKeystrokeBenchmark(testName: normalizedTestName, host: host)
        case "testOptimizedLargeEditorPaste10KB":
            return makeOptimizedLargeEditorPasteBenchmark(testName: normalizedTestName, host: host)
        case "testOptimizedLargeEditorSelectionReplace":
            return makeOptimizedLargeEditorSelectionReplaceBenchmark(testName: normalizedTestName, host: host)
        case "testImagePNGDecode":
            return makeImageDecodeBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData
            )
        case "testImageTextureUpload":
            return makeImageUploadBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData,
                host: host
            )
        case "testImageFirstVisible":
            return makeImageFirstVisibleBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData,
                host: host
            )
        case "testOptimizedImagePNGDecode":
            return makeOptimizedImageDecodeBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData
            )
        case "testOptimizedImageTextureUpload":
            return makeOptimizedImageUploadBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData,
                host: host
            )
        case "testOptimizedImageFirstVisible":
            return makeOptimizedImageFirstVisibleBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData,
                host: host
            )
        case "testButtonPressResponse":
            return makeButtonPressResponseBenchmark(
                testName: normalizedTestName,
                image: assets.checkerImage,
                host: host
            )
        case "testSliderScrubResponse":
            return makeSliderScrubResponseBenchmark(
                testName: normalizedTestName,
                image: assets.checkerImage,
                host: host
            )
        case "testTextFocusResponse":
            return makeTextFocusResponseBenchmark(testName: normalizedTestName, host: host)
        case "testSingleNodeReconcile":
            return makeReconcileMutationBenchmark(
                testName: normalizedTestName,
                dirtyNodes: 1,
                host: host
            )
        case "testTreeMutation1Pct":
            return makeReconcileMutationBenchmark(
                testName: normalizedTestName,
                dirtyNodes: 10,
                host: host
            )
        case "testTreeMutation10Pct":
            return makeReconcileMutationBenchmark(
                testName: normalizedTestName,
                dirtyNodes: 100,
                host: host
            )
        case "testThemeSwapFull":
            return makeThemeSwapFullBenchmark(testName: normalizedTestName, host: host)
        case "testEmptyRootMount":
            return makeEmptyRootMountBenchmark(testName: normalizedTestName, host: host)
        case "testFlatRects10Mount":
            return makeFlatRectMountBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testFlatRects100Mount":
            return makeFlatRectMountBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testFlatRects1000Mount":
            return makeFlatRectMountBenchmark(testName: normalizedTestName, count: 1_000, host: host)
        case "testFlatRects10Mutate":
            return makeFlatRectMutateBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testFlatRects100Mutate":
            return makeFlatRectMutateBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testFlatRects1000Mutate":
            return makeFlatRectMutateBenchmark(testName: normalizedTestName, count: 1_000, host: host)
        case "testFlatRects100RemoveAll":
            return makeFlatRectRemoveAllBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testFlatRects100Remount":
            return makeFlatRectRemountBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testOptimizedFlatRects10Mount":
            return makeOptimizedFlatRectMountBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testOptimizedFlatRects100Mount":
            return makeOptimizedFlatRectMountBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testOptimizedFlatRects1000Mount":
            return makeOptimizedFlatRectMountBenchmark(testName: normalizedTestName, count: 1_000, host: host)
        case "testOptimizedFlatRects10Mutate":
            return makeOptimizedFlatRectMutateBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testOptimizedFlatRects100Mutate":
            return makeOptimizedFlatRectMutateBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testOptimizedFlatRects1000Mutate":
            return makeOptimizedFlatRectMutateBenchmark(testName: normalizedTestName, count: 1_000, host: host)
        case "testOptimizedLabels10Mount":
            return makeOptimizedLabelMountBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testOptimizedLabels100Mount":
            return makeOptimizedLabelMountBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testOptimizedLabels1000Mount":
            return makeOptimizedLabelMountBenchmark(testName: normalizedTestName, count: 1_000, host: host)
        case "testOptimizedLabels10Mutate":
            return makeOptimizedLabelMutateBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testOptimizedLabels100Mutate":
            return makeOptimizedLabelMutateBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testOptimizedLabels1000Mutate":
            return makeOptimizedLabelMutateBenchmark(testName: normalizedTestName, count: 1_000, host: host)
        case "testLabels10Mount":
            return makeLabelMountBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testLabels100Mount":
            return makeLabelMountBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testLabels1000Mount":
            return makeLabelMountBenchmark(testName: normalizedTestName, count: 1_000, host: host)
        case "testLabels10Mutate":
            return makeLabelMutateBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testLabels100Mutate":
            return makeLabelMutateBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testLabels1000Mutate":
            return makeLabelMutateBenchmark(testName: normalizedTestName, count: 1_000, host: host)
        case "testOptimizedCards10Mount":
            return makeOptimizedCardMountBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testOptimizedCards100Mount":
            return makeOptimizedCardMountBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testOptimizedCards10Mutate":
            return makeOptimizedCardMutateBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testOptimizedCards100Mutate":
            return makeOptimizedCardMutateBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testCards10Mount":
            return makeCardMountBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testCards100Mount":
            return makeCardMountBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testCards10Mutate":
            return makeCardMutateBenchmark(testName: normalizedTestName, count: 10, host: host)
        case "testCards100Mutate":
            return makeCardMutateBenchmark(testName: normalizedTestName, count: 100, host: host)
        case "testOptimizedImages10Mount":
            return makeOptimizedImageMountBenchmark(
                testName: normalizedTestName,
                count: 10,
                image: assets.checkerImage,
                host: host
            )
        case "testOptimizedImages100Mount":
            return makeOptimizedImageMountBenchmark(
                testName: normalizedTestName,
                count: 100,
                image: assets.checkerImage,
                host: host
            )
        case "testOptimizedImages10Mutate":
            return makeOptimizedImageMutateBenchmark(
                testName: normalizedTestName,
                count: 10,
                image: assets.checkerImage,
                host: host
            )
        case "testOptimizedImages100Mutate":
            return makeOptimizedImageMutateBenchmark(
                testName: normalizedTestName,
                count: 100,
                image: assets.checkerImage,
                host: host
            )
        case "testImages10Mount":
            return makeImageMountBenchmark(
                testName: normalizedTestName,
                count: 10,
                image: assets.checkerImage,
                host: host
            )
        case "testImages100Mount":
            return makeImageMountBenchmark(
                testName: normalizedTestName,
                count: 100,
                image: assets.checkerImage,
                host: host
            )
        case "testImages10Mutate":
            return makeImageMutateBenchmark(
                testName: normalizedTestName,
                count: 10,
                image: assets.checkerImage,
                host: host
            )
        case "testImages100Mutate":
            return makeImageMutateBenchmark(
                testName: normalizedTestName,
                count: 100,
                image: assets.checkerImage,
                host: host
            )
        case "testControlSetMount":
            return makeControlSetMountBenchmark(
                testName: normalizedTestName,
                image: assets.checkerImage,
                host: host
            )
        case "testControlSetMutate":
            return makeControlSetMutateBenchmark(
                testName: normalizedTestName,
                image: assets.checkerImage,
                host: host
            )
        case "testSpinnerSpin":
            let view = SpinnerBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 32, height: 32))
            var phase: CGFloat = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                phase = (phase + 0.03125).truncatingRemainder(dividingBy: 1.0)
                view.phase = phase
                host.commit(view)
            }
        case "testOptimizedSpinnerSpin":
            let view = OptimizedSpinnerBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 32, height: 32))
            var phase: CGFloat = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                phase = (phase + 0.03125).truncatingRemainder(dividingBy: 1.0)
                view.phase = phase
                host.commit(view)
            }
        case "testProgressIndeterminate":
            let view = ProgressBarBenchView(frame: .zero)
            view.progress = nil
            host.mount(view, size: CGSize(width: 280, height: 16))
            var phase: CGFloat = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                phase = (phase + 0.0275).truncatingRemainder(dividingBy: 1.0)
                view.progress = nil
                view.phase = phase
                host.commit(view)
            }
        case "testOptimizedProgressIndeterminate":
            let view = OptimizedProgressBarBenchView(frame: .zero)
            view.progress = nil
            host.mount(view, size: CGSize(width: 280, height: 16))
            var phase: CGFloat = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                phase = (phase + 0.0275).truncatingRemainder(dividingBy: 1.0)
                view.progress = nil
                view.phase = phase
                host.commit(view)
            }
        case "testButtonPressScale":
            let button = UIButton(type: .system)
            button.setTitle("Tap", for: .normal)
            button.configuration = .filled()
            host.mount(button, size: CGSize(width: 120, height: 40))
            var scale: CGFloat = 0.98
            var delta: CGFloat = 0.004
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 64)
            {
                scale += delta
                if scale >= 1.0 || scale <= 0.98
                {
                    delta = -delta
                }
                button.transform = CGAffineTransform(scaleX: scale, y: scale)
                host.commit(button)
            }
        case "testOptimizedButtonPressScale":
            let button = OptimizedButtonBenchView(frame: .zero)
            button.scale = 0.98
            button.setNeedsDisplay()
            host.mount(button, size: CGSize(width: 120, height: 40))
            var scale: CGFloat = 0.98
            var delta: CGFloat = 0.004
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 64)
            {
                scale += delta
                if scale >= 1.0 || scale <= 0.98
                {
                    delta = -delta
                }
                button.scale = scale
                host.commit(button)
            }
        case "testToggleThumbSpring":
            let view = ToggleBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 48, height: 24))
            var phase: CGFloat = 0.0
            var velocity: CGFloat = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                let target: CGFloat = 1.0
                let stiffness: CGFloat = 20.0
                let damping: CGFloat = 2.0 * sqrt(stiffness)
                let dt: CGFloat = 0.016
                let acceleration = stiffness * (target - phase) - damping * velocity
                velocity += acceleration * dt
                phase += velocity * dt
                if abs(target - phase) < 0.001 && abs(velocity) < 0.001
                {
                    phase = 0.0
                    velocity = 0.0
                }
                view.phase = phase
                host.commit(view)
            }
        case "testOptimizedToggleThumbSpring":
            let view = OptimizedToggleBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 48, height: 24))
            var phase: CGFloat = 0.0
            var velocity: CGFloat = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                let target: CGFloat = 1.0
                let stiffness: CGFloat = 20.0
                let damping: CGFloat = 2.0 * sqrt(stiffness)
                let dt: CGFloat = 0.016
                let acceleration = stiffness * (target - phase) - damping * velocity
                velocity += acceleration * dt
                phase += velocity * dt
                if abs(target - phase) < 0.001 && abs(velocity) < 0.001
                {
                    phase = 0.0
                    velocity = 0.0
                }
                view.phase = phase
                host.commit(view)
            }
        case "testSliderThumbMove":
            let slider = UISlider(frame: .zero)
            host.mount(slider, size: CGSize(width: 240, height: 32))
            var value: Float = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                value += 0.01
                if value > 1.0
                {
                    value = 0.0
                }
                slider.value = value
                host.commit(slider)
            }
        case "testOptimizedSliderThumbMove":
            let slider = OptimizedSliderBenchView(frame: .zero)
            host.mount(slider, size: CGSize(width: 240, height: 32))
            var value = CGFloat(0.0)
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                value += 0.01
                if value > 1.0
                {
                    value = 0.0
                }
                slider.value = value
                host.commit(slider)
            }
        case "testImageZoomPan":
            let imageView = UIImageView(image: assets.checkerImage)
            imageView.contentMode = .scaleAspectFit
            host.mount(imageView, size: CGSize(width: 260, height: 200))
            var scale: CGFloat = 1.0
            var offset = CGPoint.zero
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                scale = min(scale + 0.01, 2.0)
                offset.x += 0.6
                offset.y -= 0.3
                imageView.transform = CGAffineTransform.identity
                    .translatedBy(x: offset.x, y: offset.y)
                    .scaledBy(x: scale, y: scale)
                if scale >= 2.0
                {
                    scale = 1.0
                    offset = .zero
                }
                host.commit(imageView)
            }
        case "testOptimizedImageZoomPan":
            let imageView = OptimizedImageTransformBenchView(frame: .zero, image: assets.checkerImage)
            host.mount(imageView, size: CGSize(width: 260, height: 200))
            var scale: CGFloat = 1.0
            var offset = CGPoint.zero
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                scale = min(scale + 0.01, 2.0)
                offset.x += 0.6
                offset.y -= 0.3
                imageView.scale = scale
                imageView.offset = offset
                if scale >= 2.0
                {
                    scale = 1.0
                    offset = .zero
                }
                host.commit(imageView)
            }
        case "testAnimTimelineBars":
            let view = TimelineBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 420, height: 220))
            var phase: CGFloat = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                phase = (phase + 0.016).truncatingRemainder(dividingBy: 1.0)
                view.phase = phase
                host.commit(view)
            }
        case "testOptimizedAnimTimelineBars":
            let view = OptimizedTimelineBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 420, height: 220))
            var phase: CGFloat = 0.0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                phase = (phase + 0.016).truncatingRemainder(dividingBy: 1.0)
                view.phase = phase
                host.commit(view)
            }
        case "testInputFormJourney":
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                step += 1
                let view = FormJourneyBenchView(frame: .zero)
                host.mount(view, size: CGSize(width: 560, height: 280))
                view.runJourney(step: step)
                host.commit(view)
            }
        case "testOptimizedInputFormJourney":
            return makeOptimizedInputFormJourneyBenchmark(testName: normalizedTestName, host: host)
        case "testCollectionNavigationJourney":
            var anchor = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 18)
            {
                anchor = (anchor + 3) % 24
                let view = CollectionBenchView(frame: .zero)
                host.mount(view, size: CGSize(width: 360, height: 240))
                withPerfSignpost("scroll")
                {
                    view.select(item: anchor)
                    host.commit(view)
                    view.select(item: anchor + 3)
                    host.commit(view)
                    view.select(item: anchor + 6)
                    host.commit(view)
                    view.select(item: anchor + 2)
                    host.commit(view)
                }
            }
        case "testOptimizedCollectionNavigationJourney":
            return makeOptimizedCollectionNavigationBenchmark(
                testName: normalizedTestName,
                host: host
            )
        case "testFeedScrollJourney":
            let view = CollectionBenchView(frame: .zero, mode: .feed)
            host.mount(view, size: CGSize(width: 360, height: 640))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 12)
            {
                withPerfSignpost("scroll")
                {
                    for phase in [CGFloat(0.0), 0.14, 0.36, 0.82, 0.56, 0.18]
                    {
                        view.scroll(to: phase)
                        host.commit(view)
                    }
                }
            }
        case "testOptimizedFeedScrollJourney":
            return makeOptimizedCollectionScrollBenchmark(
                testName: normalizedTestName,
                mode: .feed,
                phases: [CGFloat(0.0), 0.14, 0.36, 0.82, 0.56, 0.18],
                host: host
            )
        case "testThumbnailGridScrollJourney":
            let view = CollectionBenchView(frame: .zero, mode: .thumbnailGrid)
            host.mount(view, size: CGSize(width: 360, height: 640))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 12)
            {
                withPerfSignpost("scroll")
                {
                    for phase in [CGFloat(0.0), 0.18, 0.42, 0.88, 0.52, 0.10]
                    {
                        view.scroll(to: phase)
                        host.commit(view)
                    }
                }
            }
        case "testOptimizedThumbnailGridScrollJourney":
            return makeOptimizedCollectionScrollBenchmark(
                testName: normalizedTestName,
                mode: .thumbnailGrid,
                phases: [CGFloat(0.0), 0.18, 0.42, 0.88, 0.52, 0.10],
                host: host
            )
        case "testChatThreadScrollJourney":
            let view = CollectionBenchView(frame: .zero, mode: .chat)
            host.mount(view, size: CGSize(width: 360, height: 640))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 12)
            {
                withPerfSignpost("scroll")
                {
                    for phase in [CGFloat(0.0), 0.12, 0.32, 0.72, 0.48, 0.08]
                    {
                        view.scroll(to: phase)
                        host.commit(view)
                    }
                }
            }
        case "testOptimizedChatThreadScrollJourney":
            return makeOptimizedCollectionScrollBenchmark(
                testName: normalizedTestName,
                mode: .chat,
                phases: [CGFloat(0.0), 0.12, 0.32, 0.72, 0.48, 0.08],
                host: host
            )
        case "testZoomImageGestureJourney":
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                step += 1
                let imageView = UIImageView(image: assets.checkerImage)
                imageView.contentMode = .scaleAspectFit
                host.mount(imageView, size: CGSize(width: 280, height: 220))

                let scale = 1.0 + CGFloat(step % 6) * 0.12
                let tx = CGFloat((step % 5) - 2) * 12.0
                let ty = CGFloat((step % 4) - 2) * -9.0
                imageView.transform = CGAffineTransform.identity
                    .translatedBy(x: tx, y: ty)
                    .scaledBy(x: scale, y: scale)
                host.commit(imageView)

                imageView.transform = .identity
                host.commit(imageView)
            }
        case "testOptimizedZoomImageGestureJourney":
            return makeOptimizedZoomImageGestureJourneyBenchmark(
                testName: normalizedTestName,
                image: assets.checkerImage,
                host: host
            )
        case "testOrchestrationJourney":
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 20)
            {
                step += 1
                let view = OrchestrationBenchView(frame: .zero)
                host.mount(view, size: CGSize(width: 300, height: 280))

                withPerfSignpost("transition")
                {
                    view.phase = 0.25
                    host.commit(view)
                    view.phase = 0.50
                    host.commit(view)
                    view.phase = 0.75
                    host.commit(view)
                    view.phase = 1.0
                    host.commit(view)
                }
                view.showingModal = step % 2 == 0
                host.commit(view)
                view.showingModal = false
                host.commit(view)
            }
        case "testOptimizedOrchestrationJourney":
            return makeOptimizedOrchestrationJourneyBenchmark(testName: normalizedTestName, host: host)
        case "testTextFieldsEditCycle":
            let view = AuthoringTextFieldsBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 320, height: 308))
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 24)
            {
                step += 1
                view.runEditCycle(step: step)
                host.commit(view)
            }
        case "testPopupWheelPickerInteraction":
            let view = PopupWheelPickerBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 300, height: 260))
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 72)
            {
                step += 1
                view.runInteraction(step: step)
                host.commit(view)
            }
        case "testBurstEmitterSample":
            let view = BurstEmitterBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 260, height: 220))
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 40)
            {
                step += 1
                view.runSample(step: step)
                host.commit(view)
            }
        case "testSurfaceRouterCompose":
            let view = SurfaceRouterComposeBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 280, height: 280))
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 32)
            {
                step += 1
                view.runComposition(step: step)
                host.commit(view)
            }
        case "testOpenCloseHeavyScreen100x":
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 1)
            {
                for index in 0..<100
                {
                    let heavy = CollectionBenchView(frame: .zero, mode: .feed)
                    host.mount(heavy, size: CGSize(width: 360, height: 640))
                    heavy.scroll(to: CGFloat(index % 5) * 0.18)
                    host.commit(heavy)
                }
            }
        case "testOptimizedOpenCloseHeavyScreen100x":
            return makeOptimizedOpenCloseHeavyScreenBenchmark(testName: normalizedTestName, host: host)
        case "testTabSwitchHeavy500x":
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 1)
            {
                for index in 0..<500
                {
                    let view: UIView
                    let size: CGSize
                    switch index % 3
                    {
                    case 0:
                        view = CollectionBenchView(frame: .zero, mode: .feed)
                        size = CGSize(width: 360, height: 640)
                    case 1:
                        view = CollectionBenchView(frame: .zero, mode: .thumbnailGrid)
                        size = CGSize(width: 360, height: 640)
                    default:
                        view = OrchestrationBenchView(frame: .zero)
                        size = CGSize(width: 300, height: 280)
                    }
                    host.mount(view, size: size)
                    host.commit(view)
                }
            }
        case "testOptimizedTabSwitchHeavy500x":
            return makeOptimizedTabSwitchHeavyBenchmark(
                testName: normalizedTestName,
                image: assets.checkerImage,
                host: host
            )
        case "testIdleAnimation600Frames":
            let view = TimelineBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 420, height: 220))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 1)
            {
                for frame in 0..<600
                {
                    view.phase = CGFloat(frame % 120) / 120.0
                    host.commit(view)
                }
            }
        case "testOptimizedIdleAnimation600Frames":
            return makeOptimizedIdleAnimationBenchmark(testName: normalizedTestName, host: host)
        case "testFlatRects10000Mount":
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 1)
            {
                let view = FlatRectGridBenchView(frame: .zero)
                view.install(count: 10_000, palettePhase: 0)
                host.mount(view, size: CGSize(width: 420, height: 760))
            }
        case "testStress300Animations":
            let view = StressBarsBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 360, height: 360))
            var step = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 20)
            {
                step += 1
                view.runPhase(step: step)
                host.commit(view)
            }
        case "testTicker100Hz":
            let view = StressBarsBenchView(frame: .zero)
            host.mount(view, size: CGSize(width: 360, height: 360))
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 1)
            {
                for tick in 0..<100
                {
                    view.runPhase(step: tick)
                    host.commit(view)
                }
            }
        case "testPermissionCallbackBridge":
            let bridge = PermissionBenchBridge(domain: "camera", status: .authorized)
            let callbackSum = UInt64Box()
            _ = bridge.subscribe(domain: "camera")
            {
                state in
                callbackSum.value &+= state.status.rawValue &+ 1
            }
            _ = bridge.subscribe(domain: "camera")
            {
                state in
                callbackSum.value &+= state.status.rawValue &+ 3
            }
            _ = bridge.subscribe(domain: "camera")
            {
                state in
                callbackSum.value &+= state.status.rawValue &+ 5
            }
            var tick: UInt64 = 0
            var limited = false
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 128)
            {
                withPerfSignpost("native.bridge")
                {
                    tick &+= 17
                    limited.toggle()
                    bridge.update(
                        domain: "camera",
                        status: limited ? .limited : .authorized,
                        timestampMs: tick
                    )
                    callbackSum.value &+= bridge.status(for: "camera").rawValue
                }
            }
        case "testOptimizedPermissionCallbackBridge":
            return makeOptimizedPermissionCallbackBenchmark(testName: normalizedTestName)
        case "testSensorLocationBridge":
            let bridge = SensorLocationBenchBridge(historyLimit: 12)
            bridge.updatePermission(authorized: true)
            var tick: UInt64 = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                withPerfSignpost("native.bridge")
                {
                    tick &+= 17
                    bridge.handleLocation(
                        BenchLocationSample(
                            latitudeDeg: 37.7749 + Double(tick) * 0.000001,
                            longitudeDeg: -122.4194 - Double(tick) * 0.000001,
                            timestampMs: tick
                        )
                    )
                    let snapshot = bridge.snapshot()
                    _ = UInt64(snapshot.historyCount) &+ (snapshot.last?.timestampMs ?? 0)
                }
            }
        case "testOptimizedSensorLocationBridge":
            return makeOptimizedSensorLocationBridgeBenchmark(testName: normalizedTestName)
        case "testBluetoothCacheBridge":
            let bridge = BluetoothBenchBridge(cacheLimit: 24)
            bridge.updatePermission(authorized: true)
            bridge.handleStateChanged(poweredOn: true)
            var tick: UInt64 = 0
            return OxideUIKitBenchmark(testName: normalizedTestName, iterations: 96)
            {
                withPerfSignpost("native.bridge")
                {
                    tick &+= 23
                    let device = BenchBluetoothDevice(
                        id: 10_000 + (tick % 12),
                        lastSeenMs: tick,
                        rssiDbm: -44
                    )
                    bridge.handleDiscovery(device)
                    let snapshot = bridge.snapshot()
                    _ = UInt64(snapshot.deviceCount) &+ (snapshot.poweredOn ? 1 : 0)
                }
            }
        case "testOptimizedBluetoothCacheBridge":
            return makeOptimizedBluetoothCacheBridgeBenchmark(testName: normalizedTestName)
        case "testPhotoImportThumbnailBridge":
            return makePhotoImportThumbnailBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData,
                host: host
            )
        case "testOptimizedPhotoImportThumbnailBridge":
            return makeOptimizedPhotoImportThumbnailBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData,
                host: host
            )
        case "testFileImportRenderBridge":
            return makeFileImportRenderBenchmark(testName: normalizedTestName, host: host)
        case "testOptimizedFileImportRenderBridge":
            return makeOptimizedFileImportRenderBenchmark(testName: normalizedTestName, host: host)
        case "testSharePayloadPrepareBridge":
            return makeSharePayloadPrepareBenchmark(testName: normalizedTestName, host: host)
        case "testOptimizedSharePayloadPrepareBridge":
            return makeOptimizedSharePayloadPrepareBenchmark(testName: normalizedTestName, host: host)
        case "testLocalJSONTransportRenderBridge":
            return makeLocalJSONTransportRenderBenchmark(testName: normalizedTestName, host: host)
        case "testOptimizedLocalJSONTransportRenderBridge":
            return makeOptimizedLocalJSONTransportRenderBenchmark(testName: normalizedTestName, host: host)
        case "testLocalImageTransportRenderBridge":
            return makeLocalImageTransportRenderBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData,
                host: host
            )
        case "testOptimizedLocalImageTransportRenderBridge":
            return makeOptimizedLocalImageTransportRenderBenchmark(
                testName: normalizedTestName,
                pngData: assets.checkerPNGData,
                host: host
            )
        default:
            return nil
        }
    }
}
