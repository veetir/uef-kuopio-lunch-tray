import Foundation

struct CacheStore {
    private let directory: URL

    init(fileManager: FileManager = .default) {
        let base = fileManager.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? fileManager.temporaryDirectory
        directory = base.appendingPathComponent("CompassLunch", isDirectory: true)
    }

    func load(restaurantCode: String, language: AppLanguage) -> MenuSnapshot? {
        let url = cacheURL(restaurantCode: restaurantCode, language: language)
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(MenuSnapshot.self, from: data)
    }

    func save(_ snapshot: MenuSnapshot) {
        do {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            let data = try JSONEncoder().encode(snapshot)
            try data.write(
                to: cacheURL(
                    restaurantCode: snapshot.restaurantCode,
                    language: snapshot.language
                ),
                options: .atomic
            )
        } catch {
            // A cache failure must never prevent the menu from being shown.
        }
    }

    private func cacheURL(restaurantCode: String, language: AppLanguage) -> URL {
        directory.appendingPathComponent("\(restaurantCode)-\(language.rawValue).json")
    }
}
