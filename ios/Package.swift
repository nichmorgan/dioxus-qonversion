// swift-tools-version:5.9

import PackageDescription

// Keep the product name in sync with src/native/ios_plugin.rs.
// Declared static so an app can link this package directly. Dioxus CLI 0.7+
// rewrites `type: .static` to `type: .dynamic` when it embeds the framework.
let package = Package(
    name: "DioxusQonversionHost",
    platforms: [
        .iOS(.v15),
    ],
    products: [
        .library(
            name: "DioxusQonversionHost",
            type: .static,
            targets: ["DioxusQonversionHost"]
        ),
    ],
    dependencies: [
        .package(url: "https://github.com/qonversion/qonversion-ios-sdk.git", from: "6.13.0"),
    ],
    targets: [
        .target(
            name: "DioxusQonversionHost",
            dependencies: [
                // This product includes Qonversion, QonversionSwift, and NoCodes.
                .product(name: "Qonversion", package: "qonversion-ios-sdk"),
            ],
            path: ".",
            sources: ["DioxusQonversionHost.swift"]
        ),
    ]
)
