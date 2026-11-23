// SPDX-License-Identifier: GPL-3.0-or-later
#include "external_palette.h"

#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QFileSystemWatcher>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QRegularExpression>
#include <QtCore/QStandardPaths>
#include <QtCore/QTimer>
#include <QtQml/QQmlPropertyMap>
#include <QtQml/qqml.h>

namespace {
class PaletteLoader final : public QObject {
public:
    explicit PaletteLoader(QQmlPropertyMap *map) : QObject(map), map_(map)
    {
        map_->insert("valid", false);
        map_->insert("dark", true);
        map_->insert("colors", QVariantMap{});
        config_ = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
        if (config_.isEmpty())
            return;
        path_ = config_ + QStringLiteral("/nectarlink/noctalia-colors.json");
        debounce_.setSingleShot(true);
        debounce_.setInterval(60);
        connect(&watcher_, &QFileSystemWatcher::fileChanged, &debounce_, qOverload<>(&QTimer::start));
        connect(&watcher_, &QFileSystemWatcher::directoryChanged, &debounce_, qOverload<>(&QTimer::start));
        connect(&debounce_, &QTimer::timeout, this, [this] { reload(); });
        reload();
    }

private:
    void reload()
    {
        // Directory watches survive atomic replacement. Watching the config
        // directory also notices creation of the nectarlink subdirectory.
        for (const auto &dir : {config_, QFileInfo(path_).absolutePath()})
            if (QFileInfo::exists(dir) && !watcher_.directories().contains(dir))
                watcher_.addPath(dir);
        if (QFileInfo::exists(path_) && !watcher_.files().contains(path_))
            watcher_.addPath(path_);

        QFile file(path_);
        constexpr qint64 maxBytes = 64 * 1024;
        const auto bytes = file.open(QIODevice::ReadOnly) ? file.read(maxBytes + 1) : QByteArray{};
        const auto document = QJsonDocument::fromJson(bytes);
        const auto root = document.object();
        const auto mode = root.value("mode").toString();
        const auto colors = root.value("colors").toObject();
        const QRegularExpression hex(QStringLiteral("^#[0-9a-fA-F]{6}$"));
        QVariantMap validated;
        bool valid = bytes.size() <= maxBytes && document.isObject()
            && (mode == "dark" || mode == "light");
        for (const char *role : {"primary", "onPrimary", "primaryContainer", "onPrimaryContainer",
                 "secondaryContainer", "onSecondaryContainer", "surface", "surfaceContainerLow",
                 "surfaceContainer", "surfaceContainerHigh", "surfaceContainerHighest",
                 "onSurface", "onSurfaceVariant", "outline", "outlineVariant", "warning", "error", "onError"}) {
            const auto value = colors.value(QLatin1String(role));
            valid = valid && value.isString() && hex.match(value.toString()).hasMatch();
            validated.insert(QLatin1String(role), value.toString());
        }
        if (valid) {
            map_->insert("colors", validated);
            map_->insert("dark", mode == "dark");
        }
        map_->insert("valid", valid);
    }

    QQmlPropertyMap *map_;
    QString config_, path_;
    QFileSystemWatcher watcher_;
    QTimer debounce_;
};

QObject *makePalette(QQmlEngine *, QJSEngine *)
{
    auto *map = QQmlPropertyMap::create();
    new PaletteLoader(map);
    return map;
}
} // namespace

void register_external_palette()
{
    static const int type = qmlRegisterSingletonType<QQmlPropertyMap>(
        "app.nectarlink.theme", 1, 0, "ExternalPalette", makePalette);
    Q_UNUSED(type);
}
