import Darwin
import Foundation

let applePrCanonicalScenarioIDs = [
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
]

struct ApplePrPlanSpec: Codable, Equatable
{
   let schemaVersion: UInt32
   let id: String
   let platform: String
   let tier: String
   let acquisition: BenchmarkArtifactIdentity
   let budget: BenchmarkArtifactIdentity
   let comparatorAudits: [ApplePrComparatorAuditBinding]
   let scenarios: [ApplePrPlanScenario]

   func validate() throws
   {
      guard schemaVersion == 1,
            id == "apple-pr",
            platform == "apple",
            tier == "pr",
            acquisition.path == "acquisition/apple-pr.json",
            budget.path == "budgets/apple-pr.json",
            benchmarkCanonicalSHA256(acquisition.sha256),
            benchmarkCanonicalSHA256(budget.sha256),
            !comparatorAudits.isEmpty,
            comparatorAudits.map(\.sortKey) == comparatorAudits.map(\.sortKey).sorted(),
            Set(comparatorAudits.map(\.sortKey)).count == comparatorAudits.count,
            scenarios.map(\.id) == applePrCanonicalScenarioIDs else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      for binding in comparatorAudits
      {
         guard binding.identity.isComplete,
               binding.audit.path.hasPrefix("audits/"),
               binding.audit.path.hasSuffix(".json"),
               benchmarkCanonicalSHA256(binding.audit.sha256) else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
      }
      for scenario in scenarios
      {
         guard scenario.artifact.path == "scenarios/\(scenario.id).json",
               benchmarkCanonicalSHA256(scenario.artifact.sha256) else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
      }
   }

   func validate(acquisition: ApplePrAcquisitionSpec) throws
   {
      try validate()
      guard acquisition.id == id,
            acquisition.platform == platform,
            acquisition.tier == tier,
            acquisition.selectedScenarioIds == scenarios.map(\.id) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
   }

   func scenarioArtifact(id: String) throws -> BenchmarkArtifactIdentity
   {
      guard let scenario = scenarios.first(where: {$0.id == id}) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      return scenario.artifact
   }
}

struct ApplePrComparatorIdentity: Codable, Equatable
{
   let platform: String
   let framework: String
   let implementation: String
   let variant: String

   var isComplete: Bool
   {
      !platform.isEmpty && !framework.isEmpty && !implementation.isEmpty && !variant.isEmpty
   }

   var sortKey: String
   {
      [platform, framework, implementation, variant].joined(separator: "\u{0}")
   }
}

struct ApplePrComparatorAuditBinding: Codable, Equatable
{
   let identity: ApplePrComparatorIdentity
   let audit: BenchmarkArtifactIdentity

   var sortKey: String
   {
      identity.sortKey
   }
}

struct ApplePrPlanScenario: Codable, Equatable
{
   let id: String
   let artifact: BenchmarkArtifactIdentity
}

private struct ApplePrBoundIdentity: Codable, Equatable
{
   let schemaVersion: UInt32
   let id: String
   let platform: String
   let tier: String
}

struct ApplePrAcquisitionSpec: Codable, Equatable
{
   let schemaVersion: UInt32
   let id: String
   let platform: String
   let tier: String
   let buildForTestingCount: UInt32
   let publicXctestMethods: [String]
   let selectedScenarioIds: [String]
   let packs: [ApplePrAcquisitionPack]
   let controllerChunks: [ApplePrControllerChunk]
   let installBudgetSeconds: UInt64
   let correctnessBudgetSeconds: UInt64
   let artifactPullCount: UInt32
   let artifactPullSecondsEach: UInt64
   let reserveSeconds: UInt64
   let hardTotalSeconds: UInt64

   func validate() throws
   {
      guard schemaVersion == 1,
            id == "apple-pr",
            platform == "apple",
            tier == "pr",
            buildForTestingCount == 1,
            publicXctestMethods == [
               "ComparisonControllerUITests.testManifestCampaign",
               "ComparisonControllerUITests.testLaunchCampaign",
            ],
            selectedScenarioIds == applePrCanonicalScenarioIDs,
            packs.count == 2,
            controllerChunks.count == 4,
            hardTotalSeconds == 906,
            hardTotalSeconds <= 1_200 else
      {
         throw BenchmarkCampaignFailure.invalidAcquisition
      }
      guard let dynamic = packs.first(where: {$0.id == "pr-non-launch"}),
            let launch = packs.first(where: {$0.id == "pr-launch"}),
            dynamic.orderedScenarioIds == dynamic.resetSegments.flatMap(\.orderedScenarioIds),
            launch.orderedScenarioIds == [applePrCanonicalScenarioIDs[0]],
            dynamic.resetSegments == [
               ApplePrResetSegment(id: "core-interaction", orderedScenarioIds: ["dashboard.mixed-static", "chat.live-update", "navigation.modal"]),
               ApplePrResetSegment(id: "scroll-damage", orderedScenarioIds: ["feed.variable-scroll"]),
               ApplePrResetSegment(id: "media-text-warm", orderedScenarioIds: ["image.decode-zoom"]),
            ],
            launch.resetSegments.isEmpty,
            dynamic.resetSegments.flatMap(\.orderedScenarioIds).count == dynamic.orderedScenarioIds.count,
            Set(dynamic.resetSegments.flatMap(\.orderedScenarioIds)) == Set(dynamic.orderedScenarioIds),
            dynamic.resetSegments.count == Int(dynamic.resetCountPerSide),
            launch.resetSegments.count == Int(launch.resetCountPerSide),
            dynamic.computedSideSeconds == dynamic.sideSeconds,
            dynamic.computedCampaignSeconds == 480,
            launch.computedSideSeconds == launch.sideSeconds,
            launch.computedCampaignSeconds == 120 else
      {
         throw BenchmarkCampaignFailure.invalidAcquisition
      }
      let chunkContract: [(String, String, String, [String], [UInt32])] = [
         ("correctness", "ComparisonControllerUITests.testManifestCampaign", "correctness", ["pr-non-launch", "pr-launch"], []),
         ("presentation-pairs-0-1", "ComparisonControllerUITests.testManifestCampaign", "minimal-presentation", ["pr-non-launch"], [0, 1]),
         ("presentation-pairs-2-3", "ComparisonControllerUITests.testManifestCampaign", "minimal-presentation", ["pr-non-launch"], [2, 3]),
         ("launch-pairs-0-3", "ComparisonControllerUITests.testLaunchCampaign", "canonical-launch", ["pr-launch"], [0, 1, 2, 3]),
      ]
      for (chunk, expected) in zip(controllerChunks, chunkContract)
      {
         guard chunk.id == expected.0,
               chunk.xctestMethod == expected.1,
               chunk.passId == expected.2,
               chunk.packIds == expected.3,
               chunk.orderedPairIndices == expected.4,
               chunk.passId != "lean",
               chunk.artifactPullAfter else
         {
            throw BenchmarkCampaignFailure.invalidAcquisition
         }
      }
   }
}

struct ApplePrAcquisitionPack: Codable, Equatable
{
   let id: String
   let orderedScenarioIds: [String]
   let resetSegments: [ApplePrResetSegment]
   let pairCount: UInt32
   let sidesPerPair: UInt32
   let resetCountPerSide: UInt32
   let resetSeconds: UInt64
   let setupSecondsPerScenario: UInt64
   let warmupSecondsPerScenario: UInt64
   let measureSecondsPerScenario: UInt64
   let sideSeconds: UInt64

   var computedSideSeconds: UInt64
   {
      UInt64(resetCountPerSide) * resetSeconds
         + UInt64(orderedScenarioIds.count) * (setupSecondsPerScenario + warmupSecondsPerScenario + measureSecondsPerScenario)
   }

   var computedCampaignSeconds: UInt64
   {
      sideSeconds * UInt64(pairCount) * UInt64(sidesPerPair)
   }
}

struct ApplePrResetSegment: Codable, Equatable
{
   let id: String
   let orderedScenarioIds: [String]
}

struct BenchmarkPackedScenario: Equatable
{
   let scenarioID: String
   let resetSegment: ApplePrResetSegment?
}

func benchmarkPackedScenarios(_ pack: ApplePrAcquisitionPack) throws -> [BenchmarkPackedScenario]
{
   guard !pack.resetSegments.isEmpty else
   {
      return pack.orderedScenarioIds.map {BenchmarkPackedScenario(scenarioID: $0, resetSegment: nil)}
   }
   guard pack.orderedScenarioIds == pack.resetSegments.flatMap(\.orderedScenarioIds) else
   {
      throw BenchmarkCampaignFailure.invalidPack(pack.id)
   }
   return pack.resetSegments.flatMap
   {
      segment in
      segment.orderedScenarioIds.enumerated().map
      {
         index, scenarioID in
         BenchmarkPackedScenario(scenarioID: scenarioID, resetSegment: index == 0 ? segment : nil)
      }
   }
}

struct ApplePrControllerChunk: Codable, Equatable
{
   let id: String
   let xctestMethod: String
   let passId: String
   let packIds: [String]
   let orderedPairIndices: [UInt32]
   let maxOccupiedSeconds: UInt64
   let artifactPullAfter: Bool
}

let appleNightlyCampaignScenarioIDs = [
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
   "idle.steady",
   "endurance.churn",
]

let appleReleaseCampaignScenarioIDs = [
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "grid.large-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
   "effects.layers",
   "mutation.damage",
   "text.multilingual",
   "resize.theme",
   "idle.steady",
   "endurance.churn",
]

enum AppleCampaignTier: String, Codable, Equatable
{
   case pr
   case nightly
   case releaseCore = "release-core"
   case claimComplete = "claim-complete"
   case extended
   case fullAttribution = "full-attribution"
}

enum AppleCampaignPassRole: String, Codable, Equatable
{
   case correctness
   case primary
   case launch
   case idle
   case endurance
   case energy
   case attribution
}

enum AppleCampaignEvidenceRole: String, Codable, Equatable
{
   case correctnessOnly = "correctness-only"
   case claimBearing = "claim-bearing"
   case descriptiveDiagnostic = "descriptive-diagnostic"
}

enum AppleCampaignBudgetComponent: String, Codable, CaseIterable, Equatable
{
   case correctnessInstallPulls = "correctness-install-pulls"
   case primaryDynamicPresentation = "primary-dynamic-presentation"
   case launchOrStartupDelivery = "launch-or-startup-delivery"
   case idleEndurance = "idle-endurance"
   case energy
   case attribution
}

struct AppleCampaignScenarioBinding: Codable, Equatable
{
   let id: String
   let expectedPath: String
   let artifact: BenchmarkArtifactIdentity?
}

enum AppleCampaignMeasurementTimingSpec: Codable, Equatable
{
   case duration(seconds: UInt64)
   case iterations(phaseID: String, sourceCount: UInt32, count: UInt32, occupiedSeconds: UInt64)

   private enum CodingKeys: String, CodingKey
   {
      case mode
      case durationSeconds
      case phaseId
      case sourceIterationCount
      case iterationCount
      case occupiedSeconds
   }

   private enum Mode: String, Codable
   {
      case duration
      case iterations
   }

   init(from decoder: Decoder) throws
   {
      let values = try decoder.container(keyedBy: CodingKeys.self)
      switch try values.decode(Mode.self, forKey: .mode)
      {
      case .duration:
         self = .duration(seconds: try values.decode(UInt64.self, forKey: .durationSeconds))
      case .iterations:
         self = .iterations(
            phaseID: try values.decode(String.self, forKey: .phaseId),
            sourceCount: try values.decode(UInt32.self, forKey: .sourceIterationCount),
            count: try values.decode(UInt32.self, forKey: .iterationCount),
            occupiedSeconds: try values.decode(UInt64.self, forKey: .occupiedSeconds)
         )
      }
   }

   func encode(to encoder: Encoder) throws
   {
      var values = encoder.container(keyedBy: CodingKeys.self)
      switch self
      {
      case .duration(let seconds):
         try values.encode(Mode.duration, forKey: .mode)
         try values.encode(seconds, forKey: .durationSeconds)
      case .iterations(let phaseID, let sourceCount, let count, let occupiedSeconds):
         try values.encode(Mode.iterations, forKey: .mode)
         try values.encode(phaseID, forKey: .phaseId)
         try values.encode(sourceCount, forKey: .sourceIterationCount)
         try values.encode(count, forKey: .iterationCount)
         try values.encode(occupiedSeconds, forKey: .occupiedSeconds)
      }
   }

   var occupiedSeconds: UInt64
   {
      switch self
      {
      case .duration(let seconds): return seconds
      case .iterations(_, _, _, let seconds): return seconds
      }
   }
}

struct AppleCampaignScenarioTimingSpec: Codable, Equatable
{
   let scenarioId: String
   let setupSeconds: UInt64
   let warmupSeconds: UInt64
   let measurement: AppleCampaignMeasurementTimingSpec
}

struct AppleCampaignPassTimingSpec: Codable, Equatable
{
   let passId: String
   let resetSecondsPerSession: UInt64
   let readinessTimeoutSeconds: UInt64
   let scenarios: [AppleCampaignScenarioTimingSpec]
}

struct AppleCampaignTimingSpec: Codable, Equatable
{
   let passes: [AppleCampaignPassTimingSpec]
}

struct AppleCampaignPackSpec: Codable, Equatable
{
   let id: String
   let orderedScenarioIds: [String]
   let isolatedProcess: Bool
}

struct AppleCampaignPassSpec: Codable, Equatable
{
   let id: String
   let role: AppleCampaignPassRole
   let evidenceRole: AppleCampaignEvidenceRole
   let pairCount: UInt32
   let packIds: [String]
   let scenarioIds: [String]
   let launchClasses: [String]
   let collector: String?
}

struct AppleCampaignBudgetComponentSpec: Codable, Equatable
{
   let component: AppleCampaignBudgetComponent
   let occupiedSeconds: UInt64
   let passIds: [String]
}

struct AppleCampaignPlanSpec: Codable, Equatable
{
   let schemaVersion: UInt32
   let id: String
   let platform: String
   let tier: AppleCampaignTier
   let budgetId: String
   let timing: AppleCampaignTimingSpec
   let scenarios: [AppleCampaignScenarioBinding]
   let packs: [AppleCampaignPackSpec]
   let passes: [AppleCampaignPassSpec]
   let budgetComponents: [AppleCampaignBudgetComponentSpec]

   func validate() throws
   {
      guard schemaVersion == 1,
            platform == "apple",
            tier != .pr,
            !id.isEmpty,
            id == budgetId,
            !scenarios.isEmpty,
            !packs.isEmpty,
            !passes.isEmpty else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let scenarioIDs = scenarios.map(\.id)
      guard benchmarkUniqueNonemptyStrings(scenarioIDs) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      for scenario in scenarios
      {
         guard scenario.expectedPath == "scenarios/\(scenario.id).json",
               let artifact = scenario.artifact,
               artifact.path == scenario.expectedPath,
               benchmarkCanonicalSHA256(artifact.sha256) else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
      }
      let scenarioSet = Set(scenarioIDs)
      let packIDs = packs.map(\.id)
      guard benchmarkUniqueNonemptyStrings(packIDs) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      for pack in packs
      {
         guard benchmarkUniqueNonemptyStrings(pack.orderedScenarioIds),
               !pack.orderedScenarioIds.isEmpty,
               Set(pack.orderedScenarioIds).isSubset(of: scenarioSet) else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
      }
      let packSet = Set(packIDs)
      let passIDs = passes.map(\.id)
      guard benchmarkUniqueNonemptyStrings(passIDs) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      for pass in passes
      {
         guard pass.pairCount > 0 || pass.role == .correctness,
               pass.packIds.allSatisfy(packSet.contains),
               pass.scenarioIds.allSatisfy(scenarioSet.contains),
               benchmarkValidAppleCampaignPassIdentity(pass),
               (pass.role == .attribution) == (pass.collector?.isEmpty == false),
               (pass.role == .launch) == !pass.launchClasses.isEmpty else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
      }
      try validateTimingContract()
      guard budgetComponents.count == AppleCampaignBudgetComponent.allCases.count,
            Set(budgetComponents.map(\.component)).count == budgetComponents.count else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      var ownedPassIDs = Set<String>()
      for component in budgetComponents
      {
         guard component.occupiedSeconds > 0 || component.passIds.isEmpty else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         for passID in component.passIds
         {
            guard passIDs.contains(passID), ownedPassIDs.insert(passID).inserted else
            {
               throw BenchmarkCampaignFailure.invalidPlan
            }
         }
      }
      guard ownedPassIDs == Set(passIDs) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      try validateTierContract()
   }

   func pass(id: String) throws -> AppleCampaignPassSpec
   {
      guard let pass = passes.first(where: {$0.id == id}) else
      {
         throw BenchmarkCampaignFailure.invalidPack(id)
      }
      return pass
   }

   private func validateTierContract() throws
   {
      let scenarioIDs = scenarios.map(\.id)
      let attributionIDs = passes.filter {$0.role == .attribution}.map(\.id)
      switch tier
      {
      case .nightly:
         guard id == "nightly-apple",
               scenarioIDs == appleNightlyCampaignScenarioIDs,
               passes.filter({$0.role == .primary}).map(\.id) == ["primary-presentation"],
               passes.filter({$0.role == .launch}).map(\.id) == ["canonical-launch"],
               attributionIDs == ["attribution-time-profiler", "attribution-physical-footprint"] else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
      case .releaseCore, .claimComplete:
         let expectedID = tier == .releaseCore ? "apple-release-core" : "apple-release-claim-complete"
         guard id == expectedID,
               scenarioIDs == appleReleaseCampaignScenarioIDs,
               packs.map(\.id) == ["launch", "core-interaction", "scroll-damage", "media-text-warm", "soak-idle", "soak-endurance"],
               attributionIDs == ["attribution-time-profiler", "attribution-system-trace", "common-gpu", "attribution-physical-footprint"] else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
      case .extended, .fullAttribution:
         guard scenarioIDs == appleReleaseCampaignScenarioIDs else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         if tier == .fullAttribution
         {
            let attribution = passes.filter {$0.role == .attribution}
            guard attribution.filter({$0.id == "full-attribution"}).count == 1,
                  attribution.allSatisfy({$0.scenarioIds == appleReleaseCampaignScenarioIDs}) else
            {
               throw BenchmarkCampaignFailure.invalidPlan
            }
         }
      case .pr:
         throw BenchmarkCampaignFailure.invalidPlan
      }
   }

   private func validateTimingContract() throws
   {
      let timedPasses = passes.filter {$0.role == .primary || $0.role == .attribution || $0.role == .idle || $0.role == .endurance}
      guard timing.passes.count == timedPasses.count,
            benchmarkUniqueNonemptyStrings(timing.passes.map(\.passId)) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let readiness: UInt64
      let warmup: UInt64
      let duration: UInt64
      let navigationIterations: UInt32
      switch tier
      {
      case .nightly: (readiness, warmup, duration, navigationIterations) = (20, 3, 12, 10)
      case .releaseCore, .claimComplete: (readiness, warmup, duration, navigationIterations) = (30, 5, 20, 20)
      case .extended, .fullAttribution: (readiness, warmup, duration, navigationIterations) = (0, 0, 0, 0)
      case .pr: throw BenchmarkCampaignFailure.invalidPlan
      }
      for pass in timedPasses
      {
         guard let overlay = timing.passes.first(where: {$0.passId == pass.id}),
               overlay.scenarios.map(\.scenarioId) == pass.scenarioIds,
               benchmarkUniqueNonemptyStrings(overlay.scenarios.map(\.scenarioId)),
               overlay.resetSecondsPerSession > 0,
               overlay.readinessTimeoutSeconds > 0 else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         for scenario in overlay.scenarios
         {
            guard scenario.setupSeconds > 0,
                  scenario.warmupSeconds > 0,
                  scenario.measurement.occupiedSeconds > 0 else
            {
               throw BenchmarkCampaignFailure.invalidPlan
            }
            if case .iterations(let phaseID, let sourceCount, let count, _) = scenario.measurement,
               phaseID.isEmpty || sourceCount == 0 || count == 0
            {
               throw BenchmarkCampaignFailure.invalidPlan
            }
            let expectedWarmup = pass.role == .idle ? 3 : (pass.role == .endurance ? 2 : warmup)
            let expectedDuration = pass.role == .idle ? 60 : (pass.role == .endurance ? 300 : duration)
            guard tier == .extended || tier == .fullAttribution || (
               overlay.resetSecondsPerSession == 5
               && overlay.readinessTimeoutSeconds == readiness
               && scenario.setupSeconds == 1
               && scenario.warmupSeconds == expectedWarmup
               && scenario.measurement == expectedTimingMeasurement(
                  scenarioID: scenario.scenarioId,
                  duration: expectedDuration,
                  navigationIterations: navigationIterations
               )
            ) else
            {
               throw BenchmarkCampaignFailure.invalidPlan
            }
         }
      }
   }
}

func expectedTimingMeasurement(scenarioID: String, duration: UInt64, navigationIterations: UInt32) -> AppleCampaignMeasurementTimingSpec
{
   if scenarioID == "navigation.modal"
   {
      return .iterations(phaseID: "canonical-cycles", sourceCount: 4, count: navigationIterations, occupiedSeconds: duration)
   }
   if scenarioID == "resize.theme"
   {
      return .iterations(phaseID: "ten-changes", sourceCount: 10, count: 10, occupiedSeconds: duration)
   }
   return .duration(seconds: duration)
}

struct AppleCampaignExecutionSelection: Equatable
{
   let pass: AppleCampaignPassSpec
   let pack: AppleCampaignPackSpec?
   let timing: AppleCampaignPassTimingSpec?
   let scenarioBindings: [AppleCampaignScenarioBinding]
}

func loadAppleCampaignPlan(loader: BenchmarkSpecLoader, identity: BenchmarkArtifactIdentity) throws -> AppleCampaignPlanSpec
{
   guard identity.path.hasPrefix("plans/"),
         identity.path.hasSuffix(".json"),
         benchmarkCanonicalSHA256(identity.sha256) else
   {
      throw BenchmarkCampaignFailure.invalidPlanHash
   }
   let data: Data
   do
   {
      data = try loader.read(identity)
   }
   catch BenchmarkContractFailure.artifactHashMismatch
   {
      throw BenchmarkCampaignFailure.invalidPlanHash
   }
   let decoder = JSONDecoder()
   decoder.keyDecodingStrategy = .convertFromSnakeCase
   let plan = try decoder.decode(AppleCampaignPlanSpec.self, from: data)
   try plan.validate()
   for binding in plan.scenarios
   {
      guard let artifact = binding.artifact else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let scenario = try loader.loadScenario(artifact)
      guard scenario.id == binding.id else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
   }
   return plan
}

func selectAppleCampaignExecution(plan: AppleCampaignPlanSpec, passID: String, requestedPackID: String?) throws -> AppleCampaignExecutionSelection
{
   try plan.validate()
   let pass = try plan.pass(id: passID)
   let pack: AppleCampaignPackSpec?
   if pass.packIds.isEmpty
   {
      guard requestedPackID == nil else
      {
         throw BenchmarkCampaignFailure.invalidPack(requestedPackID ?? "missing")
      }
      pack = nil
   }
   else
   {
      let selectedPackID: String
      if let requestedPackID
      {
         selectedPackID = requestedPackID
      }
      else if pass.packIds.count == 1, let onlyPackID = pass.packIds.first
      {
         selectedPackID = onlyPackID
      }
      else
      {
         throw BenchmarkCampaignFailure.invalidPack("missing")
      }
      guard pass.packIds.contains(selectedPackID),
            let selected = plan.packs.first(where: {$0.id == selectedPackID}) else
      {
         throw BenchmarkCampaignFailure.invalidPack(selectedPackID)
      }
      pack = selected
   }
   let orderedScenarioIDs = pack?.orderedScenarioIds ?? pass.scenarioIds
   let selectedScenarioIDs = orderedScenarioIDs.filter(pass.scenarioIds.contains)
   guard !selectedScenarioIDs.isEmpty,
         pack == nil || selectedScenarioIDs == orderedScenarioIDs else
   {
      throw BenchmarkCampaignFailure.invalidPack(pack?.id ?? pass.id)
   }
   let bindings = try selectedScenarioIDs.map
   {
      id in
      guard let binding = plan.scenarios.first(where: {$0.id == id}), binding.artifact != nil else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      return binding
   }
   let timing: AppleCampaignPassTimingSpec?
   if pass.role == .primary || pass.role == .attribution || pass.role == .idle || pass.role == .endurance
   {
      guard let passTiming = plan.timing.passes.first(where: {$0.passId == pass.id}) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let scenarioTimings = try selectedScenarioIDs.map
      {
         scenarioID in
         guard let scenarioTiming = passTiming.scenarios.first(where: {$0.scenarioId == scenarioID}) else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         return scenarioTiming
      }
      timing = AppleCampaignPassTimingSpec(
         passId: passTiming.passId,
         resetSecondsPerSession: passTiming.resetSecondsPerSession,
         readinessTimeoutSeconds: passTiming.readinessTimeoutSeconds,
         scenarios: scenarioTimings
      )
   }
   else
   {
      timing = nil
   }
   return AppleCampaignExecutionSelection(pass: pass, pack: pack, timing: timing, scenarioBindings: bindings)
}

private func benchmarkValidAppleCampaignPassIdentity(_ pass: AppleCampaignPassSpec) -> Bool
{
   switch pass.role
   {
   case .correctness: return pass.id == "correctness"
   case .primary: return pass.id == "primary-presentation"
   case .launch: return pass.id == "canonical-launch"
   case .idle: return pass.id == "idle"
   case .endurance: return pass.id == "endurance"
   case .energy: return pass.id == "energy"
   case .attribution:
      return [
         "attribution-time-profiler",
         "attribution-system-trace",
         "common-gpu",
         "attribution-physical-footprint",
         "full-attribution",
      ].contains(pass.id)
   }
}

private func benchmarkUniqueNonemptyStrings(_ values: [String]) -> Bool
{
   !values.isEmpty && values.allSatisfy {!$0.isEmpty} && Set(values).count == values.count
}

struct BenchmarkCampaignInvocation: Equatable
{
   let request: ComparisonProbeRequest
   let chunkID: String
   let packID: String
   let planArtifact: BenchmarkArtifactIdentity?
   let scaleOverlayArtifact: BenchmarkArtifactIdentity?

   static func commandLine() throws -> BenchmarkCampaignInvocation
   {
      let arguments = CommandLine.arguments
      func value(_ flag: String) throws -> String
      {
         guard let index = arguments.firstIndex(of: flag), index + 1 < arguments.count else
         {
            throw ComparisonContractError.missingArgument(flag)
         }
         return arguments[index + 1]
      }
      let planSHA256 = try value("-oxide-compare-plan-sha")
      let runID = try value("-oxide-compare-run-id")
      let chunkID = try value("-oxide-compare-chunk")
      let passID = try value("-oxide-compare-pass")
      let planArtifact: BenchmarkArtifactIdentity?
      if let index = arguments.firstIndex(of: "-oxide-compare-plan-path"), index + 1 < arguments.count
      {
         planArtifact = BenchmarkArtifactIdentity(path: arguments[index + 1], sha256: planSHA256)
      }
      else
      {
         planArtifact = nil
      }
      let scaleOverlayArtifact: BenchmarkArtifactIdentity?
      if let pathIndex = arguments.firstIndex(of: "-oxide-compare-scale-overlay-path"), pathIndex + 1 < arguments.count,
         let shaIndex = arguments.firstIndex(of: "-oxide-compare-scale-overlay-sha"), shaIndex + 1 < arguments.count
      {
         scaleOverlayArtifact = BenchmarkArtifactIdentity(path: arguments[pathIndex + 1], sha256: arguments[shaIndex + 1])
      }
      else
      {
         scaleOverlayArtifact = nil
      }
      let requestedPackID: String?
      if let index = arguments.firstIndex(of: "-oxide-compare-pack"), index + 1 < arguments.count
      {
         requestedPackID = arguments[index + 1]
      }
      else
      {
         requestedPackID = nil
      }
      guard let pairIndex = UInt64(try value("-oxide-compare-pair")),
            let side = ComparisonSide(rawValue: try value("-oxide-compare-side")) else
      {
         throw ComparisonContractError.invalidArgument("campaign pair or side")
      }
      let generation: String
      if let index = arguments.firstIndex(of: "-oxide-compare-generation"), index + 1 < arguments.count
      {
         generation = arguments[index + 1]
         try validateComparisonSHA256(generation)
      }
      else
      {
         generation = benchmarkCampaignGeneration(
            planSHA256: planSHA256,
            runID: runID,
            chunkID: chunkID,
            passID: passID,
            pairIndex: pairIndex,
            side: side
         )
      }
      return BenchmarkCampaignInvocation(
         request: ComparisonProbeRequest(
            schemaVersion: 1,
            runID: runID,
            planSHA256: planSHA256,
            passID: passID,
            pairIndex: pairIndex,
            side: side,
            generation: generation,
            transportMode: .perAppContainer,
            predecessorSHA256: nil
         ),
         chunkID: chunkID,
         packID: planArtifact == nil
            ? try benchmarkCampaignPackID(passID: passID, requestedPackID: requestedPackID)
            : try benchmarkGenericCampaignPackID(passID: passID, requestedPackID: requestedPackID),
         planArtifact: planArtifact,
         scaleOverlayArtifact: scaleOverlayArtifact
      )
   }
}

struct BenchmarkCorrectnessPackRun: Equatable
{
   let packID: String
   let executionIndex: UInt32
}

func benchmarkCorrectnessPackRuns(_ packIDs: [String]) throws -> [BenchmarkCorrectnessPackRun]
{
   guard packIDs == ["pr-non-launch", "pr-launch"] else
   {
      throw BenchmarkCampaignFailure.invalidPack(packIDs.joined(separator: ","))
   }
   return packIDs.enumerated().map
   {
      index, packID in
      BenchmarkCorrectnessPackRun(packID: packID, executionIndex: UInt32(index))
   }
}

func benchmarkCampaignPackID(passID: String, requestedPackID: String?) throws -> String
{
   switch passID
   {
   case "correctness":
      guard let requestedPackID,
            requestedPackID == "pr-non-launch" || requestedPackID == "pr-launch" else
      {
         throw BenchmarkCampaignFailure.invalidPack(requestedPackID ?? "missing")
      }
      return requestedPackID
   case "minimal-presentation":
      guard requestedPackID == nil || requestedPackID == "pr-non-launch" else
      {
         throw BenchmarkCampaignFailure.invalidPack(requestedPackID ?? "missing")
      }
      return "pr-non-launch"
   case "canonical-launch":
      guard requestedPackID == nil || requestedPackID == "pr-launch" else
      {
         throw BenchmarkCampaignFailure.invalidPack(requestedPackID ?? "missing")
      }
      return "pr-launch"
   default:
      throw BenchmarkCampaignFailure.invalidPack(passID)
   }
}

func benchmarkGenericCampaignPackID(passID: String, requestedPackID: String?) throws -> String
{
   switch passID
   {
   case "primary-presentation":
      guard let requestedPackID, !requestedPackID.isEmpty else
      {
         throw BenchmarkCampaignFailure.invalidPack(requestedPackID ?? "missing")
      }
      return requestedPackID
   case "idle":
      guard requestedPackID == nil || requestedPackID == "soak-idle" else
      {
         throw BenchmarkCampaignFailure.invalidPack(requestedPackID ?? "missing")
      }
      return "soak-idle"
   case "endurance":
      guard requestedPackID == nil || requestedPackID == "soak-endurance" else
      {
         throw BenchmarkCampaignFailure.invalidPack(requestedPackID ?? "missing")
      }
      return "soak-endurance"
   case "canonical-launch":
      guard requestedPackID == nil || requestedPackID == "launch" else
      {
         throw BenchmarkCampaignFailure.invalidPack(requestedPackID ?? "missing")
      }
      return "launch"
   case "correctness", "energy", "attribution-time-profiler", "attribution-system-trace", "common-gpu", "attribution-physical-footprint", "full-attribution":
      guard requestedPackID?.isEmpty != true else
      {
         throw BenchmarkCampaignFailure.invalidPack("missing")
      }
      return requestedPackID ?? "direct"
   default:
      throw BenchmarkCampaignFailure.invalidPack(passID)
   }
}

func benchmarkTelemetryCapacity(pack: ApplePrAcquisitionPack, passID: String) throws -> Int
{
   let displayRecordsPerSecond: UInt64 = passID == "common-gpu" || passID == "full-attribution" ? 600 : 240
   let (displayRecords, displayOverflow) = pack.sideSeconds.multipliedReportingOverflow(by: displayRecordsPerSecond)
   let (requiredRecords, reserveOverflow) = displayRecords.addingReportingOverflow(4_096)
   guard !displayOverflow, !reserveOverflow, requiredRecords <= UInt64(Int.max) else
   {
      throw BenchmarkCampaignFailure.invalidPack(pack.id)
   }
   var capacity = 65_536
   while UInt64(capacity) < requiredRecords
   {
      guard capacity <= Int.max / 2 else {throw BenchmarkCampaignFailure.invalidPack(pack.id)}
      capacity *= 2
   }
   return capacity
}

struct BenchmarkCheckpointEvidence: Codable, Equatable
{
   let checkpointID: String
   let actualState: BenchmarkArtifactIdentity
   let expectedStateSHA256: String
   let actualAccessibility: BenchmarkArtifactIdentity
   let expectedAccessibilitySHA256: String
   let actualRawAccessibility: BenchmarkArtifactIdentity?
   let actualGeometry: BenchmarkArtifactIdentity?
   let actualScreenshot: BenchmarkArtifactIdentity?
   let expectedScreenshotSHA256: String
   let visibleRoleCounts: [BenchmarkRoleCount]
   let validation: String
   let screenshotValidation: String
}

struct BenchmarkResetEvidence: Codable, Equatable
{
   let segmentID: String
   let orderedScenarioIDs: [String]
   let anchorScenarioID: String
   let checkpointID: String
   let beforeState: BenchmarkArtifactIdentity
   let afterState: BenchmarkArtifactIdentity
   let beforeAccessibility: BenchmarkArtifactIdentity
   let afterAccessibility: BenchmarkArtifactIdentity
   let visibleRoleCounts: [BenchmarkRoleCount]
   let quiescenceValidation: String
   let validation: String
}

struct BenchmarkResetRecoveryEvidence: Codable, Equatable
{
   let segmentID: String
   let fixedPackBaselinePhysicalFootprintBytes: UInt64
   let recoveredPhysicalFootprintBytes: UInt64
   let recoveryLimitBytes: UInt64
   let recoveryDeadlineMs: UInt64
   let validation: String
}

struct BenchmarkResetRecoveryAttemptEvidence: Codable, Equatable
{
   let schemaVersion: UInt32
   let segmentID: String
   let fixedPackBaselinePhysicalFootprintBytes: UInt64
   let initialPostTeardownPhysicalFootprintBytes: UInt64
   let recoveryLimitBytes: UInt64
   let recoveryDeadlineMs: UInt64
}

struct BenchmarkScenarioEvidence: Codable, Equatable
{
   let scenarioID: String
   let firstTelemetrySequence: UInt64
   let lastTelemetrySequence: UInt64
   let checkpointStateSHA256: [String: String]
   let checkpointAccessibilitySHA256: [String: String]
   let checkpoints: [BenchmarkCheckpointEvidence]
   let completedLogicalUpdates: UInt64
}

#if os(macOS)
struct BenchmarkMacOSSurfaceSnapshot: Codable, Equatable
{
   let windowLogicalWidthMilliPoints: UInt64
   let windowLogicalHeightMilliPoints: UInt64
   let viewportLogicalWidthMilliPoints: UInt64
   let viewportLogicalHeightMilliPoints: UInt64
   let backingPixelWidth: UInt64
   let backingPixelHeight: UInt64
   let insetTopMilliPoints: UInt64
   let insetLeftMilliPoints: UInt64
   let insetBottomMilliPoints: UInt64
   let insetRightMilliPoints: UInt64
   let backingScaleMilli: UInt64
   let finalColorFormat: String
   let finalColorSpace: String
   let alphaMode: String
   let sampleCount: UInt64
   let compositorScaling: String
   let targetRefreshPolicy: String
   let targetRefreshMillihz: UInt64
   let surfaceImplementation: String
   let internalColorFormat: String
}

struct BenchmarkMacOSSurfaceReceipt: Codable, Equatable
{
   let schemaVersion: UInt32
   let runID: String
   let planSHA256: String
   let chunkID: String
   let passID: String
   let pairIndex: UInt64
   let side: ComparisonSide
   let generation: String
   let packID: String
   let snapshot: BenchmarkMacOSSurfaceSnapshot
   let observedRefreshMillihz: UInt64?
   let observedRefreshSource: String
   let validation: String
}
#endif

struct BenchmarkCampaignEnvelope: Codable, Equatable
{
   let schemaVersion: UInt32
   let runID: String
   let planSHA256: String
   let chunkID: String
   let passID: String
   let pairIndex: UInt64
   let side: ComparisonSide
   let generation: String
   let packID: String
   let predecessorCheckpointSHA256: String?
   let telemetrySHA256: String
   let telemetryByteCount: UInt64
   let telemetryCoverage: BenchmarkArtifactIdentity?
   let surfaceReceipt: BenchmarkArtifactIdentity?
   let timebaseNumerator: UInt32
   let timebaseDenominator: UInt32
   let scenarios: [BenchmarkScenarioEvidence]
   let resets: [BenchmarkResetEvidence]
   let resetRecoveries: [BenchmarkResetRecoveryEvidence]
   let timingClaim: String
   let injectionScope: String
   let validation: String
}

struct BenchmarkCampaignReadyEnvelope: Codable, Equatable
{
   let schemaVersion: UInt32
   let runID: String
   let planSHA256: String
   let chunkID: String
   let passID: String
   let pairIndex: UInt64
   let side: ComparisonSide
   let generation: String
   let packID: String
   let scenarioIDs: [String]
   let readyTimestamp: UInt64
   let timebaseNumerator: UInt32
   let timebaseDenominator: UInt32
   let durable: Bool
}

struct BenchmarkWarmupFootprintEvidence: Codable, Equatable
{
   let schemaVersion: UInt32
   let packSamples: [[UInt64]]
   let packHighWaterBytes: UInt64
   let captureBaselineBytes: UInt64
   let fixedBaselineBytes: UInt64
   let convergenceDeltaLimitBytes: UInt64
   let convergenceBasis: String
   let requiredStableTransitions: UInt32
   let attemptLimit: UInt32
}

struct BenchmarkControllerReceipt: Codable, Equatable
{
   let schemaVersion: UInt32
   let runID: String
   let planSHA256: String
   let chunkID: String
   let passID: String
   let pairIndex: UInt64
   let side: ComparisonSide
   let generation: String
   let packID: String
   let launchT0: UInt64
   let readyTimestamp: UInt64
   let completeTimestamp: UInt64?
   let complete: Bool
   let primaryAvailability: String
}

enum BenchmarkCampaignFailure: Error, Equatable
{
   case invalidAcquisition
   case invalidPlan
   case invalidPlanHash
   case invalidPack(String)
   case checkpointMismatch(String)
   case invalidRoleCounts(String)
   case footprintUnavailable
   case packWarmupUnstable([[UInt64]])
   case captureWarmupUnstable(UInt64)
   case footprintRecoveryTimeout(UInt64, UInt64)
   case quiescenceUnavailable
   case correctnessGeometryUnavailable
   case displayLinkUnavailable
   case virtualClockUnavailable
}

func benchmarkPhysicalFootprintBytes() throws -> UInt64
{
   var info = task_vm_info_data_t()
   var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
   let result = withUnsafeMutablePointer(to: &info)
   {
      $0.withMemoryRebound(to: integer_t.self, capacity: Int(count))
      {
         task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
      }
   }
   guard result == KERN_SUCCESS else
   {
      throw BenchmarkCampaignFailure.footprintUnavailable
   }
   return info.phys_footprint
}

func benchmarkFixedFootprintRecoveryLimit(_ baseline: UInt64) throws -> UInt64
{
   let (limit, overflow) = baseline.addingReportingOverflow(baseline / 20)
   guard !overflow else {throw BenchmarkCampaignFailure.footprintUnavailable}
   return limit
}

struct BenchmarkValidatedCheckpoint
{
   let state: Data
   let accessibility: Data
   let visibleRoleCounts: [BenchmarkRoleCount]
}

func validateBenchmarkCheckpoint(_ actual: BenchmarkAdapterCheckpoint, expected: BenchmarkParityCheckpoint, loader: BenchmarkSpecLoader) throws -> BenchmarkValidatedCheckpoint
{
   let state = try benchmarkCanonicalJSON(actual.state)
   let accessibility = try benchmarkCanonicalJSON(actual.accessibility)
   let expectedState = try benchmarkCanonicalJSON(loader.read(expected.state))
   let expectedAccessibility = try benchmarkCanonicalJSON(loader.read(expected.accessibility))
   guard state == expectedState else
   {
      throw BenchmarkCampaignFailure.checkpointMismatch("\(expected.id):state")
   }
   guard accessibility == expectedAccessibility else
   {
      throw BenchmarkCampaignFailure.checkpointMismatch("\(expected.id):accessibility")
   }
   guard try benchmarkRoleCountMap(actual.visibleRoleCounts) == benchmarkRoleCountMap(expected.expectedVisibleRoleCounts) else
   {
      throw BenchmarkCampaignFailure.invalidRoleCounts(expected.id)
   }
   return BenchmarkValidatedCheckpoint(
      state: state,
      accessibility: accessibility,
      visibleRoleCounts: actual.visibleRoleCounts
   )
}

func benchmarkCanonicalJSON(_ data: Data) throws -> Data
{
   let object = try JSONSerialization.jsonObject(with: data)
   guard JSONSerialization.isValidJSONObject(object) else
   {
      throw BenchmarkCampaignFailure.checkpointMismatch("non-object-json")
   }
   return try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys, .withoutEscapingSlashes])
}

func benchmarkRoleCountMap(_ counts: [BenchmarkRoleCount]) throws -> [String: UInt32]
{
   var result = [String: UInt32]()
   for count in counts
   {
      guard !count.role.isEmpty, result[count.role] == nil else
      {
         throw BenchmarkCampaignFailure.invalidRoleCounts(count.role)
      }
      result[count.role] = count.count
   }
   return result
}

func loadApplePrPlan(loader: BenchmarkSpecLoader, expectedSHA256: String) throws -> ApplePrPlanSpec
{
   guard benchmarkCanonicalSHA256(expectedSHA256) else
   {
      throw BenchmarkCampaignFailure.invalidPlanHash
   }
   let data: Data
   do
   {
      data = try loader.read(BenchmarkArtifactIdentity(path: "plans/apple-pr.json", sha256: expectedSHA256))
   }
   catch BenchmarkContractFailure.artifactHashMismatch
   {
      throw BenchmarkCampaignFailure.invalidPlanHash
   }
   let decoder = JSONDecoder()
   decoder.keyDecodingStrategy = .convertFromSnakeCase
   let plan = try decoder.decode(ApplePrPlanSpec.self, from: data)
   try plan.validate()
   _ = try loader.read(plan.acquisition)
   let budget = try decoder.decode(ApplePrBoundIdentity.self, from: loader.read(plan.budget))
   guard budget == ApplePrBoundIdentity(schemaVersion: 1, id: plan.id, platform: plan.platform, tier: plan.tier) else
   {
      throw BenchmarkCampaignFailure.invalidPlan
   }
   for scenario in plan.scenarios
   {
      _ = try loader.read(scenario.artifact)
   }
   for binding in plan.comparatorAudits
   {
      _ = try loader.read(binding.audit)
   }
   return plan
}

func loadApplePrAcquisition(loader: BenchmarkSpecLoader, identity: BenchmarkArtifactIdentity) throws -> ApplePrAcquisitionSpec
{
   let decoder = JSONDecoder()
   decoder.keyDecodingStrategy = .convertFromSnakeCase
   let acquisition = try decoder.decode(ApplePrAcquisitionSpec.self, from: loader.read(identity))
   try acquisition.validate()
   return acquisition
}

private func benchmarkCanonicalSHA256(_ value: String) -> Bool
{
   value.utf8.count == 64 && value.utf8.allSatisfy {($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102)}
}

func benchmarkCampaignPairOrder(_ pairIndex: UInt32) -> [ComparisonSide]
{
   pairIndex % 2 == 0 ? [.oxide, .native] : [.native, .oxide]
}

func benchmarkCampaignGeneration(planSHA256: String, runID: String, chunkID: String, passID: String, pairIndex: UInt64, side: ComparisonSide) -> String
{
   comparisonSHA256(Data("\(planSHA256)\u{0}\(runID)\u{0}\(chunkID)\u{0}\(passID)\u{0}\(pairIndex)\u{0}\(side.rawValue)".utf8))
}
