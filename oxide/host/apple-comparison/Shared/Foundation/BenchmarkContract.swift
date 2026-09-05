import CryptoKit
import CoreText
import Foundation
import QuartzCore

struct BenchmarkArtifactIdentity: Codable, Equatable
{
   let path: String
   let sha256: String
}

struct BenchmarkFontPackIdentity: Codable, Equatable
{
   let id: String
   let manifest: String
   let sha256: String
}

struct BenchmarkAssetManifest: Codable, Equatable
{
   let schemaVersion: UInt32
   let id: String
   let artifacts: [BenchmarkAssetFile]
   let inlineTextAtlas: BenchmarkInlineTextAtlas?
}

struct BenchmarkAssetFile: Codable, Equatable
{
   let role: String
   let artifact: BenchmarkArtifactIdentity
   let mediaType: String
   let colorSpace: String
}

struct BenchmarkInlineTextAtlas: Codable, Equatable
{
   let columns: UInt32
   let rows: UInt32
   let sourceRepository: String
   let sourceCommit: String
   let license: BenchmarkArtifactIdentity
   let variants: [BenchmarkInlineTextAtlasVariant]
   let entries: [BenchmarkInlineTextAsset]
}

struct BenchmarkInlineTextAtlasVariant: Codable, Equatable
{
   let artifactRole: String
   let pixelWidth: UInt32
   let pixelHeight: UInt32
   let emPixels: UInt32
}

struct BenchmarkInlineTextAsset: Codable, Equatable
{
   let grapheme: String
   let column: UInt32
   let row: UInt32
   let advanceMillionths: UInt32
   let topFromBaselineMillionths: Int32
   let widthMillionths: UInt32
   let heightMillionths: UInt32
}

struct BenchmarkFontPackManifest: Codable, Equatable
{
   let schemaVersion: UInt32
   let id: String
   let fonts: [BenchmarkFontFile]
}

struct BenchmarkFontFile: Codable, Equatable
{
   let role: String
   let artifact: BenchmarkArtifactIdentity
   let variationAxes: [BenchmarkFontVariationAxis]
   let sourceRepository: String
   let sourceCommit: String
   let sourcePath: String
   let license: BenchmarkArtifactIdentity
}

struct BenchmarkFontVariationAxis: Codable, Equatable
{
   let tag: String
   let valueMillionths: Int64
}

struct BenchmarkRoleCount: Codable, Equatable
{
   let role: String
   let count: UInt32
}

enum BenchmarkMacOSComparatorScale: String, Codable
{
   case oneX = "one_x"
   case twoX = "two_x"
}

enum BenchmarkMacOSComparatorScaleDimension: String, Codable
{
   case datasetCardinality = "dataset_cardinality"
   case operationCardinality = "operation_cardinality"
}

enum BenchmarkMacOSComparatorScaleTransform: String, Codable
{
   case namespacedDatasetShards = "namespaced_dataset_shards"
   case isolatedOperationShadow = "isolated_operation_shadow"
}

struct BenchmarkMacOSComparatorScaleOverlay: Codable, Equatable
{
   let schemaVersion: UInt32
   let scenarioID: String
   let scale: BenchmarkMacOSComparatorScale
   let dimension: BenchmarkMacOSComparatorScaleDimension
   let transform: BenchmarkMacOSComparatorScaleTransform
   let baseCardinality: UInt64
   let effectiveCardinality: UInt64
   let fixture: BenchmarkArtifactIdentity

   private enum CodingKeys: String, CodingKey
   {
      case schemaVersion
      case scenarioID = "scenarioId"
      case scale
      case dimension
      case transform
      case baseCardinality
      case effectiveCardinality
      case fixture
   }
}

struct BenchmarkMacOSComparatorRuntimeAttestation: Codable, Equatable
{
   let schemaVersion: UInt32
   let scenarioID: String
   let side: String
   let scale: BenchmarkMacOSComparatorScale
   let scaleOverlaySHA256: String
   let applicationRunCount: UInt32
   let effectiveCardinality: UInt64
   let completed: Bool

   private enum CodingKeys: String, CodingKey
   {
      case schemaVersion
      case scenarioID = "scenarioId"
      case side
      case scale
      case scaleOverlaySHA256 = "scaleOverlaySha256"
      case applicationRunCount
      case effectiveCardinality
      case completed
   }
}

struct BenchmarkSceneContract: Codable, Equatable
{
   let roles: [String]
   let styleTokens: BenchmarkArtifactIdentity
   let layoutAssertions: BenchmarkArtifactIdentity
}

struct BenchmarkPhase: Codable, Equatable
{
   let id: String
   let measured: Bool
   let durationMs: UInt64?
   let trace: BenchmarkArtifactIdentity?
}

struct BenchmarkParityCheckpoint: Codable, Equatable
{
   let id: String
   let phaseId: String
   let atUs: UInt64?
   let state: BenchmarkArtifactIdentity
   let accessibility: BenchmarkArtifactIdentity
   let geometry: BenchmarkArtifactIdentity?
   let screenshot: BenchmarkArtifactIdentity?
   let expectedVisibleRoleCounts: [BenchmarkRoleCount]
}

private struct BenchmarkReleaseCandidateCheckpoint: Codable
{
   let id: String
   let phaseId: String
   let atUs: UInt64?
   let state: BenchmarkArtifactIdentity
   let accessibility: BenchmarkArtifactIdentity
   let screenshot: BenchmarkArtifactIdentity?
}

private struct BenchmarkReleaseCandidateScreenshotMaterialization: Codable
{
   let required: Bool
   let status: String
   let scenarioManifestPath: String
}

private struct BenchmarkReleaseCandidate: Codable
{
   let schemaVersion: UInt32
   let id: String
   let candidateStatus: String
   let owningPass: String
   let fixture: BenchmarkArtifactIdentity
   let assets: BenchmarkArtifactIdentity
   let fontPack: BenchmarkFontPackIdentity
   let viewportClasses: [String]
   let scene: BenchmarkSceneContract
   let phases: [BenchmarkPhase]
   let primaryMetric: String
   let parityCheckpoints: [BenchmarkReleaseCandidateCheckpoint]
   let expectedVisibleRoleCountsByCheckpoint: [String: [BenchmarkRoleCount]]
   let screenshotMaterialization: BenchmarkReleaseCandidateScreenshotMaterialization
}

struct BenchmarkFairnessContract: Codable, Equatable
{
   let locale: String
   let timezone: String
   let direction: String
   let logicalViewportWidth: UInt32
   let logicalViewportHeight: UInt32
   let expectedVisibleRoleCounts: [BenchmarkRoleCount]
   let scheduleToleranceUs: UInt64
   let coordinateToleranceMicrounits: UInt32
   let elapsedTimeDriven: Bool
}

struct BenchmarkScenario: Codable, Equatable
{
   let schemaVersion: UInt32
   let id: String
   let fixture: BenchmarkArtifactIdentity
   let assets: BenchmarkArtifactIdentity
   let fontPack: BenchmarkFontPackIdentity
   let viewportClass: String
   let scene: BenchmarkSceneContract
   let phases: [BenchmarkPhase]
   let primaryMetric: String
   let requiredMetrics: [String]
   let optionalMetrics: [String]
   let parityCheckpoints: [BenchmarkParityCheckpoint]
   let fairnessContract: BenchmarkFairnessContract
}

struct BenchmarkTraceEvent: Codable, Equatable, Hashable
{
   let atUs: UInt64
   let op: String
   let pointer: UInt32?
   let xMillionths: Int32?
   let yMillionths: Int32?
   let deltaXMillionths: Int32?
   let deltaYMillionths: Int32?
   let target: String?
   let value: BenchmarkTraceValue?
   let stateId: String?
}

enum BenchmarkTraceValue: Codable, Equatable, Hashable
{
   case boolean(Bool)
   case integer(Int64)
   case text(String)

   init(from decoder: Decoder) throws
   {
      let container = try decoder.singleValueContainer()
      if let value = try? container.decode(Bool.self)
      {
         self = .boolean(value)
      }
      else if let value = try? container.decode(Int64.self)
      {
         self = .integer(value)
      }
      else
      {
         self = .text(try container.decode(String.self))
      }
   }

   func encode(to encoder: Encoder) throws
   {
      var container = encoder.singleValueContainer()
      switch self
      {
      case .boolean(let value): try container.encode(value)
      case .integer(let value): try container.encode(value)
      case .text(let value): try container.encode(value)
      }
   }
}

enum BenchmarkContractFailure: Error, Equatable
{
   case artifactEscapesRoot(String)
   case artifactHashMismatch(String)
   case invalidFont(String)
   case invalidScenario(String)
}

private struct BenchmarkAppleFontRecord
{
   let descriptor: CTFontDescriptor
   let sourceURL: URL
   let variations: [NSNumber: NSNumber]
   let defaults: [NSNumber: NSNumber]
}

struct BenchmarkAppleFontCatalog
{
   private let records: [String: BenchmarkAppleFontRecord]

   fileprivate init(records: [String: BenchmarkAppleFontRecord])
   {
      self.records = records
   }

   func font(role: String, size: CGFloat) throws -> CTFont
   {
      guard let record = records[role], size.isFinite, size > 0 else
      {
         throw BenchmarkContractFailure.invalidFont(role)
      }
      let attributes = [kCTFontVariationAttribute as String: record.variations] as CFDictionary
      let descriptor = CTFontDescriptorCreateCopyWithAttributes(record.descriptor, attributes)
      let font = CTFontCreateWithFontDescriptor(descriptor, size, nil)
      guard let sourceURL = CTFontCopyAttribute(font, kCTFontURLAttribute) as? URL,
            sourceURL.standardizedFileURL.resolvingSymlinksInPath() == record.sourceURL,
            let applied = CTFontCopyVariation(font) as? [NSNumber: NSNumber],
            record.variations.allSatisfy(
            {
               identifier, expected in
               guard let actual = applied[identifier] ?? record.defaults[identifier] else
               {
                  return false
               }
               return abs(actual.doubleValue - expected.doubleValue) <= 0.000_000_5
            }) else
      {
         throw BenchmarkContractFailure.invalidFont(role)
      }
      return font
   }
}

struct BenchmarkSpecLoader
{
   let root: URL

   func loadScenario(relativePath: String) throws -> BenchmarkScenario
   {
      try loadScenario(data: read(relativePath: relativePath))
   }

   func loadScenario(_ artifact: BenchmarkArtifactIdentity) throws -> BenchmarkScenario
   {
      try loadScenario(data: read(artifact))
   }

   func loadReleaseCandidateCaptureScenario(id: String) throws -> BenchmarkScenario
   {
      let allowed = ["grid.large-scroll", "effects.layers", "mutation.damage", "text.multilingual", "resize.theme"]
      guard allowed.contains(id) else
      {
         throw BenchmarkContractFailure.invalidScenario("release-candidate:\(id)")
      }
      let relativePath = "release-candidates/\(id).candidate.json"
      let data = try read(relativePath: relativePath)
      let object = try JSONSerialization.jsonObject(with: data)
      var canonical = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys, .withoutEscapingSlashes])
      canonical.append(0x0a)
      guard data == canonical else
      {
         throw BenchmarkContractFailure.invalidScenario("release-candidate-canonical:\(id)")
      }
      let candidate = try Self.decoder.decode(BenchmarkReleaseCandidate.self, from: data)
      guard candidate.schemaVersion == 1,
            candidate.id == id,
            candidate.candidateStatus == "blocked-on-canonical-screenshots",
            candidate.owningPass == "minimal-presentation",
            candidate.viewportClasses.first == "phone-portrait",
            candidate.screenshotMaterialization.required,
            candidate.screenshotMaterialization.status == "blocked",
            candidate.screenshotMaterialization.scenarioManifestPath == "scenarios/\(id).json",
            !candidate.phases.isEmpty,
            !candidate.parityCheckpoints.isEmpty,
            candidate.parityCheckpoints.allSatisfy({$0.screenshot == nil}),
            Set(candidate.expectedVisibleRoleCountsByCheckpoint.keys) == Set(candidate.parityCheckpoints.map(\.id)) else
      {
         throw BenchmarkContractFailure.invalidScenario("release-candidate:\(id)")
      }
      _ = try read(candidate.fixture)
      _ = try loadAssetManifest(candidate.assets)
      _ = try loadFontPack(candidate.fontPack)
      _ = try read(candidate.scene.styleTokens)
      _ = try read(candidate.scene.layoutAssertions)
      for phase in candidate.phases
      {
         if let trace = phase.trace {_ = try read(trace)}
      }
      let phaseIDs = Set(candidate.phases.map(\.id))
      let checkpoints = try candidate.parityCheckpoints.map
      {
         checkpoint in
         guard phaseIDs.contains(checkpoint.phaseId),
               let counts = candidate.expectedVisibleRoleCountsByCheckpoint[checkpoint.id] else
         {
            throw BenchmarkContractFailure.invalidScenario("release-candidate:\(id):\(checkpoint.id)")
         }
         _ = try read(checkpoint.state)
         _ = try read(checkpoint.accessibility)
         return BenchmarkParityCheckpoint(
            id: checkpoint.id,
            phaseId: checkpoint.phaseId,
            atUs: checkpoint.atUs,
            state: checkpoint.state,
            accessibility: checkpoint.accessibility,
            geometry: nil,
            screenshot: nil,
            expectedVisibleRoleCounts: counts
         )
      }
      return BenchmarkScenario(
         schemaVersion: 1,
         id: candidate.id,
         fixture: candidate.fixture,
         assets: candidate.assets,
         fontPack: candidate.fontPack,
         viewportClass: "phone-portrait",
         scene: candidate.scene,
         phases: candidate.phases,
         primaryMetric: candidate.primaryMetric,
         requiredMetrics: [],
         optionalMetrics: [],
         parityCheckpoints: checkpoints,
         fairnessContract: BenchmarkFairnessContract(
            locale: "en_US_POSIX",
            timezone: "UTC",
            direction: id == "text.multilingual" ? "mixed-ltr-rtl" : "ltr",
            logicalViewportWidth: 390,
            logicalViewportHeight: 844,
            expectedVisibleRoleCounts: checkpoints[0].expectedVisibleRoleCounts,
            scheduleToleranceUs: 1_000,
            coordinateToleranceMicrounits: 1_000,
            elapsedTimeDriven: true
         )
      )
   }

   func loadMacOSComparatorScaleOverlay(_ artifact: BenchmarkArtifactIdentity, scenario: BenchmarkScenario) throws -> BenchmarkMacOSComparatorScaleOverlay
   {
      let overlay = try Self.decoder.decode(BenchmarkMacOSComparatorScaleOverlay.self, from: read(artifact))
      let expected = benchmarkMacOSComparatorScaleContract(scenario.id)
      let multiplier: UInt64 = overlay.scale == .oneX ? 1 : 2
      guard overlay.schemaVersion == 1,
            overlay.scenarioID == scenario.id,
            overlay.fixture == scenario.fixture,
            overlay.dimension == expected.dimension,
            overlay.transform == expected.transform,
            overlay.baseCardinality == expected.base,
            overlay.baseCardinality <= UInt64.max / multiplier,
            overlay.effectiveCardinality == overlay.baseCardinality * multiplier else
      {
         throw BenchmarkContractFailure.invalidScenario("macos-scale:\(scenario.id)")
      }
      return overlay
   }

   private func loadScenario(data: Data) throws -> BenchmarkScenario
   {
      let scenario = try Self.decoder.decode(BenchmarkScenario.self, from: data)
      try validate(scenario)
      return scenario
   }

   func loadTrace(_ artifact: BenchmarkArtifactIdentity) throws -> [BenchmarkTraceEvent]
   {
      try Self.decoder.decode([BenchmarkTraceEvent].self, from: try read(artifact))
   }

   func loadFontPack(_ identity: BenchmarkFontPackIdentity) throws -> BenchmarkFontPackManifest
   {
      let artifact = BenchmarkArtifactIdentity(path: identity.manifest, sha256: identity.sha256)
      let manifest = try Self.decoder.decode(BenchmarkFontPackManifest.self, from: read(artifact))
      guard manifest.schemaVersion == 1, manifest.id == identity.id, !manifest.fonts.isEmpty else
      {
         throw BenchmarkContractFailure.invalidScenario(identity.id)
      }
      var roles = Set<String>()
      for font in manifest.fonts
      {
         guard !font.role.isEmpty,
               roles.insert(font.role).inserted,
               !font.variationAxes.isEmpty else
         {
            throw BenchmarkContractFailure.invalidFont(font.role)
         }
         var tags = Set<String>()
         for axis in font.variationAxes
         {
            guard tags.insert(axis.tag).inserted,
                  benchmarkFontVariationIdentifier(axis.tag) != nil else
            {
               throw BenchmarkContractFailure.invalidFont(font.role)
            }
         }
         _ = try read(font.artifact)
         _ = try read(font.license)
      }
      return manifest
   }

   func loadRegisteredFontCatalog(_ identity: BenchmarkFontPackIdentity) throws -> BenchmarkAppleFontCatalog
   {
      let manifest = try loadFontPack(identity)
      var records = [String: BenchmarkAppleFontRecord]()
      records.reserveCapacity(manifest.fonts.count)
      for entry in manifest.fonts
      {
         let url = try verifiedURL(entry.artifact).standardizedFileURL.resolvingSymlinksInPath()
         var registrationError: Unmanaged<CFError>?
         if !CTFontManagerRegisterFontsForURL(url as CFURL, .process, &registrationError)
         {
            let error = registrationError?.takeRetainedValue() as Error?
            guard (error as NSError?)?.code == CTFontManagerError.alreadyRegistered.rawValue else
            {
               throw BenchmarkContractFailure.invalidFont(entry.role)
            }
         }
         guard let descriptors = CTFontManagerCreateFontDescriptorsFromURL(url as CFURL) as? [CTFontDescriptor],
               let descriptor = descriptors.first else
         {
            throw BenchmarkContractFailure.invalidFont(entry.role)
         }
         let baseFont = CTFontCreateWithFontDescriptor(descriptor, 15, nil)
         guard let supportedAxes = CTFontCopyVariationAxes(baseFont) as? [[CFString: Any]] else
         {
            throw BenchmarkContractFailure.invalidFont(entry.role)
         }
         guard supportedAxes.count == entry.variationAxes.count else
         {
            throw BenchmarkContractFailure.invalidFont(entry.role)
         }
         var supported = [NSNumber: ClosedRange<Double>]()
         var defaults = [NSNumber: NSNumber]()
         for axis in supportedAxes
         {
            guard let identifier = axis[kCTFontVariationAxisIdentifierKey] as? NSNumber,
                  let minimum = axis[kCTFontVariationAxisMinimumValueKey] as? NSNumber,
                  let maximum = axis[kCTFontVariationAxisMaximumValueKey] as? NSNumber,
                  let defaultValue = axis[kCTFontVariationAxisDefaultValueKey] as? NSNumber else
            {
               throw BenchmarkContractFailure.invalidFont(entry.role)
            }
            supported[identifier] = minimum.doubleValue...maximum.doubleValue
            defaults[identifier] = defaultValue
         }
         var variations = [NSNumber: NSNumber]()
         for (index, axis) in entry.variationAxes.enumerated()
         {
            guard let identifierValue = benchmarkFontVariationIdentifier(axis.tag) else
            {
               throw BenchmarkContractFailure.invalidFont(entry.role)
            }
            let identifier = NSNumber(value: identifierValue)
            let value = Double(axis.valueMillionths) / 1_000_000
            guard value.isFinite,
                  supportedAxes[index][kCTFontVariationAxisIdentifierKey] as? NSNumber == identifier,
                  let range = supported[identifier],
                  range.contains(value) else
            {
               throw BenchmarkContractFailure.invalidFont(entry.role)
            }
            variations[identifier] = NSNumber(value: value)
         }
         records[entry.role] = BenchmarkAppleFontRecord(
            descriptor: descriptor,
            sourceURL: url,
            variations: variations,
            defaults: defaults
         )
      }
      return BenchmarkAppleFontCatalog(records: records)
   }

   func loadAssetManifest(_ identity: BenchmarkArtifactIdentity) throws -> BenchmarkAssetManifest
   {
      let manifest = try Self.decoder.decode(BenchmarkAssetManifest.self, from: read(identity))
      guard manifest.schemaVersion == 1, !manifest.id.isEmpty, !manifest.artifacts.isEmpty else
      {
         throw BenchmarkContractFailure.invalidScenario(manifest.id)
      }
      var roles = Set<String>()
      for asset in manifest.artifacts
      {
         guard !asset.role.isEmpty,
               roles.insert(asset.role).inserted,
               !asset.mediaType.isEmpty,
               !asset.colorSpace.isEmpty else
         {
            throw BenchmarkContractFailure.invalidScenario(manifest.id)
         }
         _ = try read(asset.artifact)
      }
      if let atlas = manifest.inlineTextAtlas
      {
         guard atlas.columns > 0,
               atlas.rows > 0,
               atlas.sourceRepository.hasPrefix("https://github.com/"),
               atlas.sourceCommit.count == 40,
               atlas.sourceCommit.allSatisfy({$0.isHexDigit && !$0.isUppercase}),
               !atlas.variants.isEmpty,
               !atlas.entries.isEmpty else
         {
            throw BenchmarkContractFailure.invalidScenario(manifest.id)
         }
         _ = try read(atlas.license)
         var emSizes = Set<UInt32>()
         var variantRoles = Set<String>()
         for variant in atlas.variants
         {
            let (expectedWidth, widthOverflow) = atlas.columns.multipliedReportingOverflow(by: variant.emPixels)
            let (expectedHeight, heightOverflow) = atlas.rows.multipliedReportingOverflow(by: variant.emPixels)
            guard variant.emPixels > 0,
                  emSizes.insert(variant.emPixels).inserted,
                  variantRoles.insert(variant.artifactRole).inserted,
                  !widthOverflow,
                  !heightOverflow,
                  variant.pixelWidth == expectedWidth,
                  variant.pixelHeight == expectedHeight,
                  let artifact = manifest.artifacts.first(where: {$0.role == variant.artifactRole}),
                  artifact.mediaType == "image/png",
                  artifact.colorSpace == "srgb" else
            {
               throw BenchmarkContractFailure.invalidScenario(manifest.id)
            }
         }
         var graphemes = Set<String>()
         var cells = Set<String>()
         for entry in atlas.entries
         {
            guard entry.grapheme.count == 1,
                  graphemes.insert(entry.grapheme).inserted,
                  entry.column < atlas.columns,
                  entry.row < atlas.rows,
                  cells.insert("\(entry.column):\(entry.row)").inserted,
                  entry.advanceMillionths > 0,
                  entry.widthMillionths > 0,
                  entry.heightMillionths > 0 else
            {
               throw BenchmarkContractFailure.invalidScenario(manifest.id)
            }
         }
      }
      return manifest
   }

   func verifiedURL(_ artifact: BenchmarkArtifactIdentity) throws -> URL
   {
      _ = try read(artifact)
      return root.appendingPathComponent(artifact.path)
   }

   func read(_ artifact: BenchmarkArtifactIdentity) throws -> Data
   {
      let data = try read(relativePath: artifact.path)
      guard comparisonSHA256(data) == artifact.sha256 else
      {
         throw BenchmarkContractFailure.artifactHashMismatch(artifact.path)
      }
      return data
   }

   private func read(relativePath: String) throws -> Data
   {
      let components = relativePath.split(separator: "/", omittingEmptySubsequences: false)
      guard !relativePath.hasPrefix("/"),
            !components.isEmpty,
            components.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." }) else
      {
         throw BenchmarkContractFailure.artifactEscapesRoot(relativePath)
      }
      let canonicalRoot = root.standardizedFileURL.resolvingSymlinksInPath()
      let url = root.appendingPathComponent(relativePath).standardizedFileURL.resolvingSymlinksInPath()
      guard url.path == canonicalRoot.path || url.path.hasPrefix(canonicalRoot.path + "/") else
      {
         throw BenchmarkContractFailure.artifactEscapesRoot(relativePath)
      }
      return try Data(contentsOf: url, options: [.mappedIfSafe])
   }

   private func validate(_ scenario: BenchmarkScenario) throws
   {
      guard scenario.schemaVersion == 1,
            !scenario.id.isEmpty,
            scenario.viewportClass == "phone-portrait",
            scenario.fairnessContract.elapsedTimeDriven,
            scenario.fairnessContract.logicalViewportWidth == 390,
            scenario.fairnessContract.logicalViewportHeight == 844,
            !scenario.phases.isEmpty,
            !scenario.parityCheckpoints.isEmpty else
      {
         throw BenchmarkContractFailure.invalidScenario(scenario.id)
      }
      var phaseIDs = Set<String>()
      for phase in scenario.phases
      {
         guard !phase.id.isEmpty,
               phaseIDs.insert(phase.id).inserted,
               !phase.measured || phase.durationMs != nil || phase.trace != nil else
         {
            throw BenchmarkContractFailure.invalidScenario(scenario.id)
         }
         if let trace = phase.trace
         {
            _ = try read(trace)
         }
      }
      for artifact in [scenario.fixture, scenario.scene.styleTokens, scenario.scene.layoutAssertions]
      {
         _ = try read(artifact)
      }
      _ = try loadAssetManifest(scenario.assets)
      _ = try loadFontPack(scenario.fontPack)
      for checkpoint in scenario.parityCheckpoints
      {
         guard phaseIDs.contains(checkpoint.phaseId), let screenshot = checkpoint.screenshot else
         {
            throw BenchmarkContractFailure.invalidScenario(scenario.id)
         }
         _ = try read(checkpoint.state)
         _ = try read(checkpoint.accessibility)
         if let geometry = checkpoint.geometry
         {
            _ = try read(geometry)
         }
         _ = try read(screenshot)
      }
   }

   private static let decoder: JSONDecoder =
   {
      let decoder = JSONDecoder()
      decoder.keyDecodingStrategy = .convertFromSnakeCase
      return decoder
   }()
}

protocol BenchmarkScenarioAdapter: AnyObject
{
   func prepare(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   func reset() throws
   func apply(event: BenchmarkTraceEvent) throws
   func checkpoint(id: String) throws -> BenchmarkAdapterCheckpoint
   func teardown() throws
}

#if os(macOS)
protocol BenchmarkMacOSSurfaceProvider
{
   func macOSSurfaceSnapshot() throws -> BenchmarkMacOSSurfaceSnapshot
}
#endif

#if os(macOS)
protocol BenchmarkReleaseCandidateScenarioAdapter: BenchmarkScenarioAdapter
{
   func prepareReleaseCandidate(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
}
#endif

protocol BenchmarkPreviewCapture
{
   func previewPNG() throws -> Data
}

#if os(macOS)
protocol BenchmarkOutputReadyAdapter
{
   func outputReady() throws
}
#endif

#if os(macOS)
struct BenchmarkMacOSCorrectnessCaptureProfile: Equatable
{
   let id: String
   let canonicalScale: UInt32

   static let canonical = BenchmarkMacOSCorrectnessCaptureProfile(
      id: "macos-canonical-srgb8-3x-v1",
      canonicalScale: 3
   )
}

struct BenchmarkMacOSCorrectnessGeometryEvidence: Codable, Equatable
{
   let schemaVersion: UInt32
   let coordinateSpace: String
   let captureProfile: String
   let canonicalScale: UInt32
   let source: String
   let positionTolerancePoints: Double
   let sizeTolerancePoints: Double
   let root: BenchmarkMacOSCorrectnessLogicalRect
   let nodes: [BenchmarkMacOSCorrectnessGeometryNode]
}

struct BenchmarkMacOSCorrectnessLogicalRect: Codable, Equatable
{
   let x: Double
   let y: Double
   let width: Double
   let height: Double
}

struct BenchmarkMacOSCorrectnessGeometryNode: Codable, Equatable
{
   let ordinal: UInt32
   let kind: String
   let role: String
   let identifier: String?
   let bounds: BenchmarkMacOSCorrectnessLogicalRect
   let textLineBounds: [BenchmarkMacOSCorrectnessLogicalRect]
}

protocol BenchmarkMacOSCorrectnessGeometryCapture
{
   func correctnessGeometry() throws -> BenchmarkMacOSCorrectnessGeometryEvidence
}

func benchmarkMacOSCorrectnessGeometry(rootBounds: CGRect, source: String, nodes: [BenchmarkMacOSCorrectnessGeometryNode]) throws -> BenchmarkMacOSCorrectnessGeometryEvidence
{
   let profile = BenchmarkMacOSCorrectnessCaptureProfile.canonical
   let values = [rootBounds.origin.x, rootBounds.origin.y, rootBounds.width, rootBounds.height]
   let pixelWidth = rootBounds.width * CGFloat(profile.canonicalScale)
   let pixelHeight = rootBounds.height * CGFloat(profile.canonicalScale)
   guard values.allSatisfy(\.isFinite),
         rootBounds.origin == .zero,
         rootBounds.width > 0,
         rootBounds.height > 0,
         rootBounds.width.rounded() == rootBounds.width,
         rootBounds.height.rounded() == rootBounds.height,
         pixelWidth.rounded() == pixelWidth,
         pixelHeight.rounded() == pixelHeight,
         !source.isEmpty,
         !nodes.isEmpty,
         nodes.enumerated().allSatisfy({index, node in
            node.ordinal == UInt32(index)
               && !node.kind.isEmpty
               && !node.role.isEmpty
               && node.bounds.x.isFinite
               && node.bounds.y.isFinite
               && node.bounds.width.isFinite
               && node.bounds.height.isFinite
               && node.bounds.width > 0
               && node.bounds.height > 0
               && node.textLineBounds.allSatisfy({line in
                  line.x.isFinite && line.y.isFinite && line.width.isFinite && line.height.isFinite
                     && line.width > 0 && line.height > 0
               })
         }) else
   {
      throw BenchmarkContractFailure.invalidScenario("macos-correctness-geometry")
   }
   return BenchmarkMacOSCorrectnessGeometryEvidence(
      schemaVersion: 1,
      coordinateSpace: "logical-points",
      captureProfile: profile.id,
      canonicalScale: profile.canonicalScale,
      source: source,
      positionTolerancePoints: 0,
      sizeTolerancePoints: 0,
      root: BenchmarkMacOSCorrectnessLogicalRect(
         x: Double(rootBounds.origin.x),
         y: Double(rootBounds.origin.y),
         width: Double(rootBounds.width),
         height: Double(rootBounds.height)
      ),
      nodes: nodes
   )
}
#endif

protocol BenchmarkRawAccessibilityCapture
{
   func rawAccessibilityTree() throws -> Data
}

protocol BenchmarkFrameDrivenAdapter
{
   func displayTick() throws
}

protocol BenchmarkQuiescenceAdapter
{
   func quiesce() throws
}

protocol BenchmarkRetiredResourceAdapter
{
   func drainRetiredResources() throws
}

protocol BenchmarkPassConfiguredAdapter
{
   func configure(passID: String)
}

protocol BenchmarkMacOSScaleConfiguredAdapter
{
   func configure(scaleOverlay: BenchmarkMacOSComparatorScaleOverlay, identity: BenchmarkArtifactIdentity) throws
   func scaleRuntimeAttestation() throws -> (effectiveCardinality: UInt64, completed: Bool)
}

private func benchmarkMacOSComparatorScaleContract(_ scenarioID: String) -> (dimension: BenchmarkMacOSComparatorScaleDimension, transform: BenchmarkMacOSComparatorScaleTransform, base: UInt64)
{
   switch scenarioID
   {
   case "startup.first-screen": return (.datasetCardinality, .namespacedDatasetShards, 24 * 1_024)
   case "feed.variable-scroll": return (.datasetCardinality, .namespacedDatasetShards, 2_000)
   case "grid.large-scroll": return (.datasetCardinality, .namespacedDatasetShards, 10_000)
   case "chat.live-update": return (.datasetCardinality, .namespacedDatasetShards, 5_000)
   case "mutation.damage": return (.datasetCardinality, .namespacedDatasetShards, 10_000)
   case "text.multilingual": return (.datasetCardinality, .namespacedDatasetShards, 1_000)
   case "dashboard.mixed-static": return (.operationCardinality, .isolatedOperationShadow, 20)
   case "navigation.modal": return (.operationCardinality, .isolatedOperationShadow, 4)
   case "image.decode-zoom": return (.operationCardinality, .isolatedOperationShadow, 1)
   case "effects.layers": return (.operationCardinality, .isolatedOperationShadow, 3)
   case "resize.theme": return (.operationCardinality, .isolatedOperationShadow, 10)
   case "idle.steady": return (.operationCardinality, .isolatedOperationShadow, 1)
   case "endurance.churn": return (.operationCardinality, .isolatedOperationShadow, 1_300)
   default: return (.operationCardinality, .isolatedOperationShadow, 0)
   }
}

protocol BenchmarkVirtualClockAdapter
{
   func setVirtualTimeUs(_ timeUs: UInt64) throws
}

#if os(macOS)
protocol BenchmarkDisplayLinkProvider
{
   func makeDisplayLink(target: Any, selector: Selector) -> CADisplayLink
}
#endif

struct BenchmarkAdapterCheckpoint: Equatable
{
   let state: Data
   let accessibility: Data
   let visibleRoleCounts: [BenchmarkRoleCount]
}

func benchmarkFontVariationIdentifier(_ tag: String) -> UInt32?
{
   guard tag.utf8.count == 4, tag.utf8.allSatisfy({$0 >= 0x20 && $0 <= 0x7e}) else
   {
      return nil
   }
   return tag.utf8.reduce(UInt32(0)) {($0 << 8) | UInt32($1)}
}

func benchmarkCheckpoint(scenario: BenchmarkScenario, checkpointID: String, model: [String: Any], visibleRoleCounts: [BenchmarkRoleCount]) throws -> BenchmarkAdapterCheckpoint
{
   guard let checkpoint = scenario.parityCheckpoints.first(where: {$0.id == checkpointID}) else
   {
      throw BenchmarkContractFailure.invalidScenario("(scenario.id):(checkpointID)")
   }
   let roles = visibleRoleCounts.map {["role": $0.role, "count": $0.count] as [String: Any]}
   let state: [String: Any] = [
      "schema_version": 2,
      "scenario_id": scenario.id,
      "checkpoint_id": checkpoint.id,
      "model": model,
      "visible_role_counts": roles,
   ]
   let nodes = visibleRoleCounts.enumerated().map
   {
      order, role in
      let visible = role.count > 0
      return [
         "role": role.role,
         "name": role.role,
         "value": String(role.count),
         "state": visible ? ["enabled", "visible"] : ["hidden"],
         "order": order,
         "focused": benchmarkSemanticRoleFocused(scenarioID: scenario.id, role: role.role, model: model),
         "actions": benchmarkSemanticRoleActions(role.role),
         "frame": benchmarkSemanticRoleFrame(scenarioID: scenario.id, role: role.role, model: model),
         "count": role.count,
         "visible": visible,
      ] as [String: Any]
   }
   let accessibility: [String: Any] = [
      "schema_version": 2,
      "scenario_id": scenario.id,
      "checkpoint_id": checkpoint.id,
      "root_frame": benchmarkSemanticRootFrame(scenarioID: scenario.id, model: model),
      "raw_tree_source": benchmarkSemanticTreeSource(scenarioID: scenario.id),
      "nodes": nodes,
   ]
   return BenchmarkAdapterCheckpoint(
      state: try JSONSerialization.data(withJSONObject: state, options: [.sortedKeys]),
      accessibility: try JSONSerialization.data(withJSONObject: accessibility, options: [.sortedKeys]),
      visibleRoleCounts: visibleRoleCounts
   )
}

func benchmarkSemanticRoleFocused(scenarioID: String, role: String, model: [String: Any]) -> Bool
{
   scenarioID == "chat.live-update" && role == "message" && !(model["focused_message_id"] is NSNull) && model["focused_message_id"] != nil
}

func benchmarkSemanticRoleActions(_ role: String) -> [String]
{
   switch role
   {
   case "primary-control", "control", "favorite-control", "send-control", "list-item", "dismiss-control", "back-control": return ["activate"]
   case "feed", "chat-thread": return ["scroll"]
   case "composer": return ["set-text"]
   case "image-canvas", "image": return ["pan", "zoom"]
   case "thumbnail-grid": return ["scroll"]
   case "grid-tile": return ["activate"]
   case "zoom-control": return ["increment", "decrement"]
   default: return []
   }
}

func benchmarkSemanticTreeSource(scenarioID: String) -> String
{
   switch scenarioID
   {
   case "grid.large-scroll", "effects.layers", "mutation.damage", "text.multilingual", "resize.theme":
      return "framework-neutral-release-contract"
   default:
      return "runtime-semantic-tree"
   }
}

func benchmarkSemanticRootFrame(scenarioID: String, model: [String: Any]) -> [Int]
{
   if scenarioID == "resize.theme",
      let width = model["width"] as? Int,
      let height = model["height"] as? Int
   {
      return [0, 0, width, height]
   }
   return [0, 0, 390, 844]
}

func benchmarkSemanticRoleFrame(scenarioID: String, role: String, model: [String: Any]? = nil) -> [Int]
{
   if scenarioID == "resize.theme", let model
   {
      let width = model["width"] as? Int ?? 390
      let height = model["height"] as? Int ?? 844
      let landscape = model["orientation"] as? String == "landscape"
      if role == "dashboard" {return [0, 0, width, height]}
      if role == "backdrop-region" {return landscape ? [20, 32, 804, 280] : [12, 42, 366, 586]}
      return landscape ? [24, 36, 796, 318] : [16, 48, 358, 728]
   }
   switch (scenarioID, role)
   {
   case ("startup.first-screen", "header"): return [16, 20, 358, 48]
   case ("startup.first-screen", "navigation"): return [16, 76, 358, 44]
   case ("startup.first-screen", "card"), ("startup.first-screen", "initial-image"): return [16, 132, 358, 552]
   case ("startup.first-screen", "primary-control"): return [16, 776, 358, 48]
   case ("dashboard.mixed-static", "dashboard"), ("idle.steady", "dashboard"), ("endurance.churn", "endurance"): return [0, 0, 390, 844]
   case ("dashboard.mixed-static", "backdrop-region"), ("idle.steady", "backdrop-region"), ("endurance.churn", "backdrop-region"): return [12, 42, 366, 586]
   case ("dashboard.mixed-static", _), ("idle.steady", _), ("endurance.churn", _): return [16, 48, 358, 728]
   case ("feed.variable-scroll", "navigation-bar"): return [0, 0, 390, 52]
   case ("feed.variable-scroll", _): return [0, 52, 390, 792]
   case ("chat.live-update", "chat-thread"), ("chat.live-update", "message"), ("chat.live-update", "avatar"): return [0, 52, 390, 700]
   case ("chat.live-update", "composer"): return [12, 780, 318, 48]
   case ("chat.live-update", "send-control"): return [338, 780, 40, 48]
   case ("navigation.modal", "navigation-list"), ("navigation.modal", "list-item"), ("navigation.modal", "detail"), ("navigation.modal", "back-control"): return [0, 0, 390, 844]
   case ("navigation.modal", "modal"), ("navigation.modal", "dismiss-control"): return [24, 132, 342, 580]
   case ("image.decode-zoom", "image-canvas"), ("image.decode-zoom", "image"): return [0, 52, 390, 740]
   case ("image.decode-zoom", "zoom-control"): return [16, 800, 358, 28]
   case ("grid.large-scroll", "thumbnail-grid"): return [0, 48, 390, 748]
   case ("grid.large-scroll", "grid-tile"): return [12, 60, 366, 724]
   case ("grid.large-scroll", "thumbnail"): return [20, 68, 350, 570]
   case ("grid.large-scroll", "tile-label"): return [20, 646, 350, 130]
   case ("grid.large-scroll", "detail-view"): return [0, 0, 390, 844]
   case ("grid.large-scroll", "detail-thumbnail"): return [16, 84, 358, 358]
   case ("grid.large-scroll", "detail-title"): return [16, 466, 358, 52]
   case ("grid.large-scroll", "back-control"): return [16, 24, 44, 44]
   case ("effects.layers", "effects-scene"), ("mutation.damage", "mutation-surface"), ("text.multilingual", "text-surface"): return [0, 0, 390, 844]
   case ("effects.layers", _): return [12, 48, 366, 748]
   case ("mutation.damage", _): return [8, 40, 374, 796]
   case ("text.multilingual", _): return [16, 40, 358, 788]
   default: return [0, 0, 390, 844]
   }
}
