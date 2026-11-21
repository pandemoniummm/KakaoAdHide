// KakaoAdHide for macOS - covers the ad banner of KakaoTalk for Mac.
//
//   KakaoAdHide                                  run (menu bar app)
//   KakaoAdHide --version                        print the version
//   KakaoAdHide --verify-feed <feed> <dmg>       release workflow check (see VerifyCommand)

import AppKit

let args = CommandLine.arguments
if args.count >= 2 {
  switch args[1] {
  case "--version": print(Config.version); exit(0)
  case "--verify-feed": exit(VerifyCommand.run(args))
  default: break   // e.g. -psn_... from older launchers, or -NSDocumentRevisionsDebugMode from Xcode
  }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
