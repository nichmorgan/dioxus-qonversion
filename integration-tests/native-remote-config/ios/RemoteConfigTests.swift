import XCTest
import Foundation

final class RemoteConfigTests: XCTestCase {
    private func consume(_ error: UnsafeMutablePointer<CChar>?) -> String? {
        guard let error else { return nil }
        defer { qrc_test_free_error(error) }
        return String(cString: error)
    }

    @MainActor
    func testRealRemoteConfig() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["QONVERSION_RUN_NATIVE_REMOTE_CONFIG"] == "1",
                          "Native SDK test disabled: use run.py ios --run")
        if let error = consume(qrc_test_initialize()) {
            XCTFail(error)
            return
        }
        // Async XCTest waiting leaves DispatchQueue.main free for the Swift SDK.
        let completed = expectation(description: "Rust Remote Config contract")
        DispatchQueue.global(qos: .userInitiated).async {
            if let error = self.consume(qrc_test_query()) {
                XCTFail(error)
            }
            completed.fulfill()
        }
        await fulfillment(of: [completed], timeout: 45)
    }
}
