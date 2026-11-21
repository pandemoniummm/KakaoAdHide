import Foundation
import Security
import Testing
@testable import KakaoAdHideCore

private func fixture(_ name: String) throws -> Data {
  try Data(contentsOf: Bundle.module.url(forResource: "Fixtures/\(name)", withExtension: nil)!)
}

private let repoRoot = URL(fileURLWithPath: #filePath)
  .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()

// The published v1.0.0 Windows feed was signed by the release workflow with the primary key: the macOS verifier
// must accept exactly what the Windows verifier accepts.
@Test func publishedWindowsFeedVerifies() throws {
  let f = try UpdateFeed.parse(fixture("v1.0.0-latest-windows.json"))
  #expect(f.version == "1.0.0")
  #expect(FeedVerifier.signatureValid(version: f.version, sha256: f.sha256, signature: f.signature))
  #expect(FeedVerifier.signatureValid(version: f.version, sha256: f.sha256.uppercased(), signature: f.signature))
}

@Test func tamperedFeedFails() throws {
  let f = try UpdateFeed.parse(fixture("v1.0.0-latest-windows.json"))
  #expect(!FeedVerifier.signatureValid(version: "1.0.1", sha256: f.sha256, signature: f.signature))
  var sha = Array(f.sha256)
  sha[0] = sha[0] == "0" ? "1" : "0"
  #expect(!FeedVerifier.signatureValid(version: f.version, sha256: String(sha), signature: f.signature))
  #expect(!FeedVerifier.signatureValid(version: f.version, sha256: f.sha256, signature: "not base64!"))
  #expect(!FeedVerifier.signatureValid(version: f.version, sha256: f.sha256, signature: f.signature, keys: []))
}

@Test func keysMatchWindows() throws {
  let cs = try String(contentsOf: repoRoot.appendingPathComponent("windows/src/UpdateKeys.cs"), encoding: .utf8)
  let windowsKeys = cs.components(separatedBy: "\"").filter { $0.hasPrefix("<RSAKeyValue>") }
  #expect(windowsKeys == UpdateKeys.publicKeys)
  #expect(UpdateKeys.publicKeys.allSatisfy { RSAXMLKey.secKey(fromXML: $0) != nil })
}

@Test func parseRejectsBadFeeds() {
  #expect(throws: (any Error).self) { try UpdateFeed.parse(Data("{}".utf8)) }
  let bad = #"{"version":"x","url":"u","size":1,"sha256":"a","signature":"b"}"#
  #expect(throws: UpdateError.badFeed) { try UpdateFeed.parse(Data(bad.utf8)) }
  let bom = Data([0xEF, 0xBB, 0xBF]) + Data(#"{"version":"1.2.3","url":"u","size":1,"sha256":"a","signature":"b"}"#.utf8)
  #expect((try? UpdateFeed.parse(bom))?.version == "1.2.3")
}

@Test func versions() {
  #expect(AppVersion("1.0.10")! > AppVersion("1.0.9")!)
  #expect(AppVersion("1.1")! == AppVersion("1.1.0")!)
  #expect(AppVersion("2.0.0")! > AppVersion("1.99.99")!)
  #expect(AppVersion("1.0.0-beta") == nil)
  #expect(AppVersion("") == nil)
}

@Test func urlMustBeInRepository() throws {
  var f = UpdateFeed(version: "1.1.0", url: "https://github.com/pandemoniummm/KakaoAdHide/releases/download/v1.1.0/KakaoAdHide-macos.dmg",
                     size: 1, sha256: "a", signature: "b")
  #expect(throws: Never.self) { _ = try FeedVerifier.checkURL(f) }
  f.url = "https://github.com/someone/KakaoAdHide/releases/download/v1.1.0/KakaoAdHide-macos.dmg"
  #expect(throws: UpdateError.outsideRepository) { _ = try FeedVerifier.checkURL(f) }
  f.url = "https://github.com/pandemoniummm/KakaoAdHide/releases/download.evil.com/x"
  #expect(throws: UpdateError.outsideRepository) { _ = try FeedVerifier.checkURL(f) }
}

// Whole-file verification with a throwaway key pair (the real private keys never leave CI / offline storage).
@Test func verifyFileWithTestKey() throws {
  let attrs: [CFString: Any] = [kSecAttrKeyType: kSecAttrKeyTypeRSA, kSecAttrKeySizeInBits: 2048]
  let priv = try #require(SecKeyCreateRandomKey(attrs as CFDictionary, nil))
  let pubDER = try #require(SecKeyCopyExternalRepresentation(SecKeyCopyPublicKey(priv)!, nil) as Data?)
  let (n, e) = try parsePKCS1(pubDER)
  let xml = "<RSAKeyValue><Modulus>\(n.base64EncodedString())</Modulus><Exponent>\(e.base64EncodedString())</Exponent></RSAKeyValue>"

  let file = FileManager.default.temporaryDirectory.appendingPathComponent("kah-test-\(UUID().uuidString)")
  try Data("hello dmg".utf8).write(to: file)
  defer { try? FileManager.default.removeItem(at: file) }
  let (hex, size) = try FeedVerifier.sha256Hex(ofFile: file)
  let sig = try #require(SecKeyCreateSignature(priv, .rsaSignatureMessagePKCS1v15SHA256,
                                               FeedVerifier.message(version: "1.2.0", sha256: hex) as CFData, nil) as Data?)
  let feed = UpdateFeed(version: "1.2.0", url: "u", size: size, sha256: hex, signature: sig.base64EncodedString())

  #expect(throws: Never.self) { try FeedVerifier.verify(feed, file: file, keys: [xml]) }
  #expect(throws: UpdateError.badSignature) { try FeedVerifier.verify(feed, file: file, keys: UpdateKeys.publicKeys) }
  var wrongSize = feed; wrongSize.size += 1
  #expect(throws: UpdateError.sizeMismatch) { try FeedVerifier.verify(wrongSize, file: file, keys: [xml]) }
  try Data("hello dmG".utf8).write(to: file)
  #expect(throws: UpdateError.hashMismatch) { try FeedVerifier.verify(feed, file: file, keys: [xml]) }
  #expect(throws: UpdateError.noKeys) { try FeedVerifier.verify(feed, file: file, keys: []) }
}

/// Minimal DER reader for SEQUENCE { INTEGER n, INTEGER e } (what SecKey exports for RSA public keys).
private func parsePKCS1(_ d: Data) throws -> (Data, Data) {
  var i = d.startIndex
  func len() -> Int {
    let b = Int(d[i]); i += 1
    if b < 0x80 { return b }
    var n = 0
    for _ in 0..<(b & 0x7F) { n = n << 8 | Int(d[i]); i += 1 }
    return n
  }
  guard d[i] == 0x30 else { throw UpdateError.badFeed }
  i += 1; _ = len()
  var out: [Data] = []
  for _ in 0..<2 {
    guard d[i] == 0x02 else { throw UpdateError.badFeed }
    i += 1
    let l = len()
    out.append(Data(d[i..<i + l]).drop(while: { $0 == 0 }))
    i += l
  }
  return (out[0], out[1])
}
