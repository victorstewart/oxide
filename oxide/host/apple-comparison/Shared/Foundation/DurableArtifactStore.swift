import Darwin
import Foundation

struct DurableArtifactStore
{
   let root: URL

   static func root(for mode: ComparisonTransportMode) throws -> URL
   {
      switch mode
      {
      case .appGroup:
         guard let root = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: comparisonAppGroup) else
         {
            throw ComparisonContractError.missingContainer(mode)
         }
         return root
      case .perAppContainer:
         guard let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else
         {
            throw ComparisonContractError.missingContainer(mode)
         }
         return root.appendingPathComponent("ComparisonRuns", isDirectory: true)
      }
   }

   func durableJSON<T: Encodable>(_ value: T, relativePath: String) throws -> (url: URL, sha256: String)
   {
      let data = try JSONEncoder.comparisonCanonical.encode(value)
      let destination = root.appendingPathComponent(relativePath, isDirectory: false)
      try durableWrite(data, to: destination)
      return (destination, comparisonSHA256(data))
   }

   func readJSON<T: Decodable>(_ type: T.Type, relativePath: String) throws -> T
   {
      let data = try Data(contentsOf: root.appendingPathComponent(relativePath, isDirectory: false))
      return try JSONDecoder().decode(type, from: data)
   }

   func durableWrite(_ data: Data, to destination: URL) throws
   {
      let directory = destination.deletingLastPathComponent()
      try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
      let temporary = destination.appendingPathExtension("tmp")
      let descriptor = Darwin.open(temporary.path, O_WRONLY | O_CREAT | O_TRUNC, S_IRUSR | S_IWUSR)
      guard descriptor >= 0 else
      {
         throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
      }

      var writeError: Error?
      data.withUnsafeBytes { bytes in
         var written = 0
         while written < bytes.count
         {
            let count = Darwin.write(descriptor, bytes.baseAddress!.advanced(by: written), bytes.count - written)
            if count > 0
            {
               written += count
            }
            else if count < 0 && errno == EINTR
            {
               continue
            }
            else
            {
               writeError = POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
               break
            }
         }
      }
      if writeError == nil && Darwin.fsync(descriptor) != 0
      {
         writeError = POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
      }
      if Darwin.close(descriptor) != 0 && writeError == nil
      {
         writeError = POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
      }
      if let writeError
      {
         throw writeError
      }
      if Darwin.rename(temporary.path, destination.path) != 0
      {
         throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
      }
      let directoryDescriptor = Darwin.open(directory.path, O_RDONLY)
      guard directoryDescriptor >= 0 else
      {
         throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
      }
      let syncResult = Darwin.fsync(directoryDescriptor)
      let syncError = errno
      let closeResult = Darwin.close(directoryDescriptor)
      if syncResult != 0
      {
         throw POSIXError(POSIXErrorCode(rawValue: syncError) ?? .EIO)
      }
      if closeResult != 0
      {
         throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
      }
   }
}

extension JSONEncoder
{
   static var comparisonCanonical: JSONEncoder
   {
      let encoder = JSONEncoder()
      encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
      return encoder
   }
}
