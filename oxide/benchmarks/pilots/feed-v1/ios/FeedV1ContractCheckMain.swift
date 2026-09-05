#if FEED_V1_CONTRACT_CHECK_MAIN
import Foundation

@main
enum FeedV1ContractCheckMain
{
   static func main() throws
   {
#if FEED_V1_CONTRACT_REHASH
      let first = try FeedV1Contract.materializeUnchecked()
      let second = try FeedV1Contract.materializeUnchecked()
#else
      let first = try FeedV1Contract.materializeForAudit()
      let second = try FeedV1Contract.materializeForAudit()
#endif
      let runtime = try FeedV1Contract.materialize()
      try require(runtime.rowHeightPrefixPoints == first.rowHeightPrefixPoints, "runtime prefix differs from the audited prefix")
      try require(runtime.canonicalSHA256 == FeedV1Contract.expectedCanonicalSHA256, "runtime fixture does not carry the audited canonical hash")
      try require(runtime.canonicalByteCount == FeedV1Contract.expectedCanonicalByteCount, "runtime fixture does not carry the audited canonical byte length")
      try require(first.rowHeightPrefixPoints == second.rowHeightPrefixPoints, "repeat materialization changed prefix geometry")
      try require(first.canonicalSHA256 == second.canonicalSHA256, "repeat materialization changed the canonical hash")
      try require(first.canonicalByteCount == second.canonicalByteCount, "repeat materialization changed canonical byte length")
      try require(first.rowCount == FeedV1Contract.rowCount, "row count mismatch")
      try require(first.rowHeightPrefixPoints.count == FeedV1Contract.rowCount + 1, "prefix count mismatch")
      try require(first.rowHeightPrefixPoints[0] == 0, "prefix origin is not zero")
      try require(first.row(at: -1) == nil, "negative row index was accepted")
      try require(first.row(at: first.rowCount) == nil, "row-count index was accepted")
      try require(first.rowHeightPoints(at: -1) == nil, "negative row-height index was accepted")
      try require(first.rowHeightPoints(at: first.rowCount) == nil, "row-count height index was accepted")
      try require(FeedV1Contract.completionNonceIsValid("primary-s00-p00-o0-oxide-forward-test"), "valid completion nonce was rejected")
      try require(FeedV1Contract.completionNonceIsValid(String(repeating: "a", count: 128)), "128-byte completion nonce was rejected")
      try require(!FeedV1Contract.completionNonceIsValid(""), "empty completion nonce was accepted")
      try require(!FeedV1Contract.completionNonceIsValid(String(repeating: "a", count: 129)), "129-byte completion nonce was accepted")
      try require(!FeedV1Contract.completionNonceIsValid("../escape"), "path completion nonce was accepted")
      try require(!FeedV1Contract.completionNonceIsValid("under_score"), "underscore completion nonce was accepted")
      try require(!FeedV1Contract.completionNonceIsValid("has.dot"), "period completion nonce was accepted")
      try require(!FeedV1Contract.completionNonceIsValid("unicode-é"), "non-ASCII completion nonce was accepted")

      var admissionOrder = [String]()
      let admission: (snapshot: Int, environment: String)? = feedV1CaptureEnvironmentAdmission(
         snapshot: {
            admissionOrder.append("snapshot")
            return 7
         },
         environment: {
            admissionOrder.append("environment")
            return "nominal"
         }
      )
      try require(admissionOrder == ["snapshot", "environment"], "environment admission did not snapshot transitions before querying the endpoint")
      try require(admission?.snapshot == 7 && admission?.environment == "nominal", "environment admission changed captured values")

      var componentIDs = Set<String>()
      componentIDs.reserveCapacity(first.rowCount * FeedV1ComponentKind.allCases.count)
      for index in 0 ..< first.rowCount
      {
         guard let firstRow = first.row(at: index), let secondRow = second.row(at: index) else
         {
            throw FeedV1ContractError.invariant("valid row index \(index) was rejected")
         }
         try require(firstRow == secondRow, "repeat materialization changed row content at row \(index)")
         let delta = first.rowHeightPrefixPoints[index + 1] - first.rowHeightPrefixPoints[index]
         try require(delta == firstRow.heightPoints, "prefix delta mismatch at row \(index)")
         try require(first.rowHeightPoints(at: index) == firstRow.heightPoints, "row height lookup mismatch at row \(index)")
         for kind in FeedV1ComponentKind.allCases
         {
            let id = firstRow.componentID(kind)
            try require(componentIDs.insert(id).inserted, "duplicate component ID \(id)")
            let rect = try first.componentRectPhysicalPixels(rowIndex: index, kind: kind)
            try require(rect.width > 0 && rect.height > 0, "empty component bounds for \(id)")
         }
      }
      try require(
         componentIDs.count == FeedV1Contract.rowCount * FeedV1ComponentKind.allCases.count,
         "component manifest count mismatch"
      )
      let topRecords = try first.visibleComponentRecords(contentOffsetPoints: 0)
      let bottomRecords = try first.visibleComponentRecords(contentOffsetPoints: Double(first.maximumContentOffsetPoints))
      try require(!topRecords.isEmpty && !bottomRecords.isEmpty, "frozen visible component records are empty")
      try require(
         topRecords.allSatisfy { $0.viewportClipPx.y >= 0 && $0.viewportClipPx.y + $0.viewportClipPx.height <= FeedV1Contract.viewportClipPhysicalPixels.height },
         "top component clip leaves the viewport"
      )
      try require(
         bottomRecords.allSatisfy { $0.viewportClipPx.y >= 0 && $0.viewportClipPx.y + $0.viewportClipPx.height <= FeedV1Contract.viewportClipPhysicalPixels.height },
         "bottom component clip leaves the viewport"
      )
      try require(
         bottomRecords.contains { $0.viewportClipPx.height < $0.contentRectPx.height },
         "bottom records omit the expected partial component clip"
      )
      let encodedRecord = try JSONEncoder().encode(topRecords[0])
      let encodedRecordText = String(decoding: encodedRecord, as: UTF8.self)
      try require(encodedRecordText.contains("\"row_index\""), "component JSON omits row_index")
      try require(encodedRecordText.contains("\"content_rect_px\""), "component JSON omits content_rect_px")
      try require(encodedRecordText.contains("\"viewport_clip_px\""), "component JSON omits viewport_clip_px")

      try require(first.firstRowIndex(intersectingContentY: -1) == 0, "negative content coordinate did not clamp to row zero")
      try require(first.firstRowIndex(intersectingContentY: 0) == 0, "content origin did not resolve to row zero")
      try require(
         first.firstRowIndex(intersectingContentY: first.contentExtentPoints) == FeedV1Contract.rowCount - 1,
         "content-end coordinate did not clamp to the final row"
      )
      try require(first.maximumContentOffsetPoints == first.contentExtentPoints - FeedV1Contract.surfaceHeightPoints, "maximum offset mismatch")

      let checkerZero = try FeedV1Contract.checkerRGBABytes(variant: 0)
      let checkerLast = try FeedV1Contract.checkerRGBABytes(variant: 63)
      try require(checkerZero.count == 12 * 12 * 4, "checker byte count mismatch")
      try require(checkerZero != checkerLast, "different checker variants produced identical bytes")
      do
      {
         _ = try FeedV1Contract.checkerRGBABytes(variant: 64)
         throw FeedV1ContractError.invariant("out-of-range checker variant was accepted")
      }
      catch FeedV1ContractError.invariant
      {
      }

#if !FEED_V1_CONTRACT_REHASH
      let canonicalBytes = try FeedV1Contract.canonicalBytesForAudit()
      try require(canonicalBytes.count == first.canonicalByteCount, "audit byte length mismatch")
      try require(FeedV1Hash.sha256Hex(canonicalBytes) == first.canonicalSHA256, "audit byte hash mismatch")
#endif

      print("canonical_sha256=\(first.canonicalSHA256)")
      print("canonical_byte_count=\(first.canonicalByteCount)")
      print("content_extent_points=\(first.contentExtentPoints)")
      print("maximum_content_offset_points=\(first.maximumContentOffsetPoints)")
   }

   private static func require(_ condition: @autoclosure () -> Bool, _ message: String) throws
   {
      guard condition() else
      {
         throw FeedV1ContractError.invariant(message)
      }
   }
}
#endif
