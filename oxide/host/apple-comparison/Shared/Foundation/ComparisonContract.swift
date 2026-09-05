import CryptoKit
import Foundation

let comparisonAppGroup = "group.com.oxide.comparison-bench"
let comparisonReadyNotification = "com.oxide.compare.ready"
let comparisonStartNotification = "com.oxide.compare.start"
let comparisonCompleteNotification = "com.oxide.compare.complete"
let comparisonFailedNotification = "com.oxide.compare.failed"

func comparisonGenerationNotification(_ base: String, generation: String) -> String
{
   "\(base).g\(generation)"
}

enum ComparisonSide: String, Codable
{
   case oxide
   case native
}

enum ComparisonTransportMode: String, Codable
{
   case appGroup = "app-group"
   case perAppContainer = "per-app-container"
}

struct ComparisonProbeRequest: Codable, Equatable
{
   let schemaVersion: Int
   let runID: String
   let planSHA256: String
   let passID: String
   let pairIndex: UInt64
   let side: ComparisonSide
   let generation: String
   let transportMode: ComparisonTransportMode
   let predecessorSHA256: String?

   static func commandLine() throws -> ComparisonProbeRequest
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

      func optionalValue(_ flag: String) -> String?
      {
         guard let index = arguments.firstIndex(of: flag), index + 1 < arguments.count else
         {
            return nil
         }
         return arguments[index + 1]
      }

      guard let pairIndex = UInt64(try value("-oxide-compare-pair")) else
      {
         throw ComparisonContractError.invalidArgument("-oxide-compare-pair")
      }
      guard let side = ComparisonSide(rawValue: try value("-oxide-compare-side")) else
      {
         throw ComparisonContractError.invalidArgument("-oxide-compare-side")
      }
      guard let transportMode = ComparisonTransportMode(rawValue: try value("-oxide-compare-transport")) else
      {
         throw ComparisonContractError.invalidArgument("-oxide-compare-transport")
      }
      return ComparisonProbeRequest(
         schemaVersion: 1,
         runID: try value("-oxide-compare-run-id"),
         planSHA256: try value("-oxide-compare-plan-sha"),
         passID: try value("-oxide-compare-pass"),
         pairIndex: pairIndex,
         side: side,
         generation: try value("-oxide-compare-generation"),
         transportMode: transportMode,
         predecessorSHA256: optionalValue("-oxide-compare-predecessor-sha")
      )
   }
}

struct ComparisonProbeEnvelope: Codable, Equatable
{
   let schemaVersion: Int
   let runID: String
   let planSHA256: String
   let passID: String
   let pairIndex: UInt64
   let side: ComparisonSide
   let generation: String
   let predecessorSHA256: String?
   let payloadSHA256: String
}

struct ComparisonProbeAcknowledgement: Codable, Equatable
{
   let schemaVersion: Int
   let generation: String
   let artifactSHA256: String
   let durable: Bool
}

struct ComparisonPreviewInvocation: Equatable
{
   let scenarioID: String
   let generation: String

   static func commandLine(arguments: [String] = CommandLine.arguments) throws -> ComparisonPreviewInvocation
   {
      func value(_ flag: String) throws -> String
      {
         guard let index = arguments.firstIndex(of: flag), index + 1 < arguments.count else
         {
            throw ComparisonContractError.missingArgument(flag)
         }
         return arguments[index + 1]
      }

      let generation = try value("-oxide-compare-preview-generation")
      try validateComparisonSHA256(generation)
      return ComparisonPreviewInvocation(
         scenarioID: try value("-oxide-compare-scenario"),
         generation: generation
      )
   }
}

struct ComparisonPreviewComplete: Codable, Equatable
{
   let schemaVersion: UInt32
   let generation: String
   let scenarioID: String
   let bundleID: String
   let fixtureSHA256: String
   let fontPackSHA256: String
   let previewSHA256: String
   let visibleRoleCounts: [BenchmarkRoleCount]
}

enum ComparisonContractError: Error, CustomStringConvertible
{
   case invalidArgument(String)
   case invalidGeneration
   case invalidHash
   case missingArgument(String)
   case missingContainer(ComparisonTransportMode)
   case missingPredecessor

   var description: String
   {
      switch self
      {
      case .invalidArgument(let argument): return "invalid comparison argument \(argument)"
      case .invalidGeneration: return "comparison generation mismatch"
      case .invalidHash: return "comparison artifact hash mismatch"
      case .missingArgument(let argument): return "missing comparison argument \(argument)"
      case .missingContainer(let mode): return "comparison container unavailable for \(mode.rawValue)"
      case .missingPredecessor: return "comparison predecessor artifact is missing"
      }
   }
}

func comparisonSHA256(_ data: Data) -> String
{
   SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

func validateComparisonSHA256(_ value: String) throws
{
   guard value.count == 64 && value.allSatisfy({ $0.isHexDigit && !$0.isUppercase }) else
   {
      throw ComparisonContractError.invalidHash
   }
}

func validateComparisonEnvelope(_ envelope: ComparisonProbeEnvelope, request: ComparisonProbeRequest) throws
{
   guard envelope.generation == request.generation else
   {
      throw ComparisonContractError.invalidGeneration
   }
   guard envelope.runID == request.runID,
         envelope.planSHA256 == request.planSHA256,
         envelope.passID == request.passID,
         envelope.pairIndex == request.pairIndex else
   {
      throw ComparisonContractError.invalidHash
   }
}
