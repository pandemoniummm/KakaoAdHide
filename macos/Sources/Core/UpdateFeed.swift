// The update feed (latest-macos.json) and its verification. Same format and signature as the Windows
// version's latest-windows.json (see docs/update-feed.md):
//   - the download link must point into this repository's releases,
//   - size and SHA-256 must match the feed,
//   - the feed's signature (RSA, PKCS#1 v1.5, SHA-256) over "KakaoAdHide\n<version>\n<sha256 hex>" (UTF-8) must
//     verify with one of the public keys in UpdateKeys.

import CryptoKit
import Foundation
import Security

public enum UpdateConfig {
  public static let repo = "pandemoniummm/KakaoAdHide"
  public static let feedURL = URL(string: "https://github.com/\(repo)/releases/latest/download/latest-macos.json")!
  public static let assetPrefix = "https://github.com/\(repo)/releases/download/"
  public static let releasesPage = URL(string: "https://github.com/\(repo)/releases/latest")!
  public static let maxFeedBytes = 64 * 1024
  public static let maxPackageBytes: Int64 = 50 * 1024 * 1024
}

public struct UpdateFeed: Codable, Equatable {
  public var version: String
  public var url: String
  public var size: Int64
  public var sha256: String
  public var signature: String
  public var page: String?

  public init(version: String, url: String, size: Int64, sha256: String, signature: String, page: String? = nil) {
    self.version = version; self.url = url; self.size = size; self.sha256 = sha256; self.signature = signature; self.page = page
  }

  public static func parse(_ data: Data) throws -> UpdateFeed {
    var d = data
    if d.starts(with: [0xEF, 0xBB, 0xBF]) { d = d.dropFirst(3) }   // UTF-8 BOM (PowerShell may write one)
    let f = try JSONDecoder().decode(UpdateFeed.self, from: d)
    guard AppVersion(f.version) != nil, !f.url.isEmpty, !f.sha256.isEmpty, !f.signature.isEmpty else {
      throw UpdateError.badFeed
    }
    return f
  }

  public var pageURL: URL { page.flatMap(URL.init(string:)) ?? UpdateConfig.releasesPage }
}

public enum UpdateError: Error, Equatable, CustomStringConvertible {
  case badFeed
  case outsideRepository
  case tooLarge
  case sizeMismatch
  case hashMismatch
  case badSignature
  case noKeys
  case badPackage(String)

  public var description: String {
    switch self {
    case .badFeed: return "업데이트 정보 형식이 올바르지 않습니다."
    case .outsideRepository: return "다운로드 주소가 이 저장소의 릴리스가 아닙니다."
    case .tooLarge: return "파일이 너무 큽니다."
    case .sizeMismatch: return "파일 크기가 맞지 않습니다."
    case .hashMismatch: return "SHA-256이 맞지 않습니다."
    case .badSignature: return "서명이 맞지 않습니다."
    case .noKeys: return "공개키가 없습니다."
    case .badPackage(let why): return "설치 파일을 확인하지 못했습니다: \(why)"
    }
  }
}

/// x.y.z, compared numerically.
public struct AppVersion: Comparable, CustomStringConvertible {
  public let parts: [Int]
  public init?(_ s: String) {
    let p = s.split(separator: ".", omittingEmptySubsequences: false).map { Int($0) }
    guard (2...4).contains(p.count), p.allSatisfy({ $0 != nil && $0! >= 0 }) else { return nil }
    parts = p.map { $0! } + Array(repeating: 0, count: 4 - p.count)
  }
  public static func < (a: AppVersion, b: AppVersion) -> Bool { a.parts.lexicographicallyPrecedes(b.parts) }
  public var description: String { parts.map(String.init).joined(separator: ".") }
}

public enum FeedVerifier {
  public static func message(version: String, sha256: String) -> Data {
    Data("KakaoAdHide\n\(version)\n\(sha256.lowercased())".utf8)
  }

  /// True when `signature` (base64) over the version and hash verifies with any of `keys`.
  public static func signatureValid(version: String, sha256: String, signature: String,
                                    keys: [String] = UpdateKeys.publicKeys) -> Bool {
    guard let sig = Data(base64Encoded: signature) else { return false }
    let msg = message(version: version, sha256: sha256)
    for xml in keys {
      guard let key = RSAXMLKey.secKey(fromXML: xml) else { continue }
      var err: Unmanaged<CFError>?
      if SecKeyVerifySignature(key, .rsaSignatureMessagePKCS1v15SHA256, msg as CFData, sig as CFData, &err) { return true }
    }
    return false
  }

  public static func sha256Hex(ofFile url: URL) throws -> (hex: String, size: Int64) {
    let h = try FileHandle(forReadingFrom: url)
    defer { try? h.close() }
    var hasher = SHA256()
    var size: Int64 = 0
    while let chunk = try h.read(upToCount: 1 << 20), !chunk.isEmpty {
      hasher.update(data: chunk)
      size += Int64(chunk.count)
    }
    return (hasher.finalize().map { String(format: "%02x", $0) }.joined(), size)
  }

  /// Checks that the file at `url` is exactly what the signed feed describes. Throws UpdateError otherwise.
  public static func verify(_ feed: UpdateFeed, file url: URL, keys: [String] = UpdateKeys.publicKeys) throws {
    guard !keys.isEmpty else { throw UpdateError.noKeys }
    let (hex, size) = try sha256Hex(ofFile: url)
    if feed.size != 0 && feed.size != size { throw UpdateError.sizeMismatch }
    guard hex == feed.sha256.lowercased() else { throw UpdateError.hashMismatch }
    guard signatureValid(version: feed.version, sha256: hex, signature: feed.signature, keys: keys) else {
      throw UpdateError.badSignature
    }
  }

  public static func checkURL(_ feed: UpdateFeed, prefix: String = UpdateConfig.assetPrefix) throws -> URL {
    guard feed.url.hasPrefix(prefix), let u = URL(string: feed.url) else { throw UpdateError.outsideRepository }
    return u
  }
}

/// .NET's RSA XML public key (<RSAKeyValue><Modulus/><Exponent/></RSAKeyValue>) as a SecKey.
public enum RSAXMLKey {
  public static func secKey(fromXML xml: String) -> SecKey? {
    guard let n = element("Modulus", in: xml), let e = element("Exponent", in: xml) else { return nil }
    // PKCS#1 RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }
    let der = derSequence(derInteger(n) + derInteger(e))
    let attrs: [CFString: Any] = [kSecAttrKeyType: kSecAttrKeyTypeRSA, kSecAttrKeyClass: kSecAttrKeyClassPublic]
    return SecKeyCreateWithData(der as CFData, attrs as CFDictionary, nil)
  }

  static func element(_ name: String, in xml: String) -> Data? {
    guard let a = xml.range(of: "<\(name)>"), let b = xml.range(of: "</\(name)>"), a.upperBound <= b.lowerBound else { return nil }
    return Data(base64Encoded: String(xml[a.upperBound..<b.lowerBound]).trimmingCharacters(in: .whitespacesAndNewlines))
  }

  static func derLength(_ n: Int) -> Data {
    if n < 0x80 { return Data([UInt8(n)]) }
    var bytes: [UInt8] = []
    var v = n
    while v > 0 { bytes.insert(UInt8(v & 0xFF), at: 0); v >>= 8 }
    return Data([0x80 | UInt8(bytes.count)] + bytes)
  }

  static func derInteger(_ raw: Data) -> Data {
    var v = Data(raw.drop(while: { $0 == 0 }))
    if v.isEmpty { v = Data([0]) }
    if v.first! & 0x80 != 0 { v.insert(0, at: 0) }   // keep it positive
    return Data([0x02]) + derLength(v.count) + v
  }

  static func derSequence(_ content: Data) -> Data { Data([0x30]) + derLength(content.count) + content }
}
