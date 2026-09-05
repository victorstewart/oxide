import Foundation

struct ComparisonTransport
{
   let request: ComparisonProbeRequest
   let store: DurableArtifactStore

   var pairRoot: String
   {
      "Runs/\(request.runID)/\(request.passID)/\(request.pairIndex)"
   }

   func runSide() throws
   {
      let predecessorSHA256: String?
      switch request.transportMode
      {
      case .appGroup:
         let seed = try store.readJSON(ComparisonProbeRequest.self, relativePath: "\(pairRoot)/controller.seed.json")
         guard seed.generation == request.generation else
         {
            throw ComparisonContractError.invalidGeneration
         }
         if request.side == .native
         {
            let predecessor = try store.readJSON(ComparisonProbeEnvelope.self, relativePath: "\(pairRoot)/oxide.json")
            try validateComparisonEnvelope(predecessor, request: seed)
            predecessorSHA256 = comparisonSHA256(try JSONEncoder.comparisonCanonical.encode(predecessor))
         }
         else
         {
            predecessorSHA256 = nil
         }
      case .perAppContainer:
         if request.side == .native
         {
            guard let predecessor = request.predecessorSHA256 else
            {
               throw ComparisonContractError.missingPredecessor
            }
            try validateComparisonSHA256(predecessor)
            predecessorSHA256 = predecessor
         }
         else
         {
            guard request.predecessorSHA256 == nil else
            {
               throw ComparisonContractError.invalidHash
            }
            predecessorSHA256 = nil
         }
      }

      let payload = Data("\(request.runID):\(request.passID):\(request.pairIndex):\(request.side.rawValue):\(request.generation)".utf8)
      let envelope = ComparisonProbeEnvelope(
         schemaVersion: 1,
         runID: request.runID,
         planSHA256: request.planSHA256,
         passID: request.passID,
         pairIndex: request.pairIndex,
         side: request.side,
         generation: request.generation,
         predecessorSHA256: predecessorSHA256,
         payloadSHA256: comparisonSHA256(payload)
      )
      let artifact = try store.durableJSON(envelope, relativePath: "\(pairRoot)/\(request.side.rawValue).json")
      let acknowledgement = ComparisonProbeAcknowledgement(
         schemaVersion: 1,
         generation: request.generation,
         artifactSHA256: artifact.sha256,
         durable: true
      )
      _ = try store.durableJSON(acknowledgement, relativePath: "\(pairRoot)/\(request.side.rawValue).ack.json")
   }
}
