// swift-tools-version:5.9
// KakaoAdHide for macOS. Build the app with ./build.sh (this package alone builds only the binary).
import PackageDescription

let package = Package(
  name: "KakaoAdHide",
  platforms: [.macOS(.v13)],
  targets: [
    // Update feed parsing and verification: no AppKit, unit tested.
    .target(name: "KakaoAdHideCore", path: "Sources/Core"),
    .executableTarget(name: "KakaoAdHide", dependencies: ["KakaoAdHideCore"], path: "Sources/App"),
    .testTarget(name: "KakaoAdHideCoreTests", dependencies: ["KakaoAdHideCore"], path: "Tests/CoreTests",
                resources: [.copy("Fixtures")]),
  ]
)
