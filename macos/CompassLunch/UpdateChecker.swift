import Foundation

enum MacUpdateCheckResult: Equatable {
    case latestPublished(
        currentVersion: String,
        releaseURL: URL
    )
    case updateAvailable(
        currentVersion: String,
        latestVersion: String,
        releaseURL: URL
    )
    case newerThanLatestPublished(
        currentVersion: String,
        latestVersion: String
    )
}

enum MacUpdateCheckError: LocalizedError {
    case invalidResponse
    case serverStatus(Int)
    case noPublishedRelease
    case invalidCurrentVersion(String)

    var errorDescription: String? {
        switch self {
        case .invalidResponse:
            "GitHub returned an invalid response."
        case let .serverStatus(status):
            "GitHub returned HTTP \(status)."
        case .noPublishedRelease:
            "No published macOS release was found."
        case let .invalidCurrentVersion(version):
            "The installed version is invalid: \(version)."
        }
    }
}

struct MacUpdateChecker {
    private let session: URLSession
    private let releasesURL: URL

    init(
        session: URLSession = .shared,
        releasesURL: URL = URL(
            string: "https://api.github.com/repos/veetir/uef-kuopio-lunch-tray/releases?per_page=100"
        )!
    ) {
        self.session = session
        self.releasesURL = releasesURL
    }

    func check(currentVersion: String) async throws -> MacUpdateCheckResult {
        var request = URLRequest(url: releasesURL)
        request.timeoutInterval = 10
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue(
            "LunchTray-macOS/\(currentVersion)",
            forHTTPHeaderField: "User-Agent"
        )

        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw MacUpdateCheckError.invalidResponse
        }
        guard (200..<300).contains(httpResponse.statusCode) else {
            throw MacUpdateCheckError.serverStatus(httpResponse.statusCode)
        }
        return try Self.result(currentVersion: currentVersion, releasesData: data)
    }

    static func result(
        currentVersion: String,
        releasesData: Data
    ) throws -> MacUpdateCheckResult {
        guard let current = ReleaseVersion(currentVersion) else {
            throw MacUpdateCheckError.invalidCurrentVersion(currentVersion)
        }
        let releases = try JSONDecoder().decode(
            [GitHubRelease].self,
            from: releasesData
        )
        guard let latest = releases
            .filter({ !$0.draft && !$0.prerelease })
            .compactMap({ release -> (GitHubRelease, ReleaseVersion)? in
                guard let version = ReleaseVersion(
                    tag: release.tagName,
                    prefix: "macos-v"
                ) else {
                    return nil
                }
                return (release, version)
            })
            .max(by: { $0.1 < $1.1 })
        else {
            throw MacUpdateCheckError.noPublishedRelease
        }

        let latestVersion = latest.1.description
        switch current.compare(to: latest.1) {
        case .orderedAscending:
            return .updateAvailable(
                currentVersion: current.description,
                latestVersion: latestVersion,
                releaseURL: latest.0.htmlURL
            )
        case .orderedSame:
            return .latestPublished(
                currentVersion: current.description,
                releaseURL: latest.0.htmlURL
            )
        case .orderedDescending:
            return .newerThanLatestPublished(
                currentVersion: current.description,
                latestVersion: latestVersion
            )
        }
    }
}

private struct GitHubRelease: Decodable {
    let tagName: String
    let htmlURL: URL
    let draft: Bool
    let prerelease: Bool

    enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case htmlURL = "html_url"
        case draft
        case prerelease
    }
}

private struct ReleaseVersion: Comparable, CustomStringConvertible {
    let major: Int
    let minor: Int
    let patch: Int

    init?(_ value: String) {
        let parts = value.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 3,
              let major = Int(parts[0]),
              let minor = Int(parts[1]),
              let patch = Int(parts[2]),
              major >= 0,
              minor >= 0,
              patch >= 0
        else {
            return nil
        }
        self.major = major
        self.minor = minor
        self.patch = patch
    }

    init?(tag: String, prefix: String) {
        guard tag.hasPrefix(prefix) else { return nil }
        self.init(String(tag.dropFirst(prefix.count)))
    }

    var description: String {
        "\(major).\(minor).\(patch)"
    }

    static func < (left: ReleaseVersion, right: ReleaseVersion) -> Bool {
        (left.major, left.minor, left.patch)
            < (right.major, right.minor, right.patch)
    }

    func compare(to other: ReleaseVersion) -> ComparisonResult {
        if self < other {
            return .orderedAscending
        }
        if other < self {
            return .orderedDescending
        }
        return .orderedSame
    }
}
