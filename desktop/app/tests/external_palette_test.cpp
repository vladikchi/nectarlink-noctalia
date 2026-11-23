// SPDX-License-Identifier: GPL-3.0-or-later
// Standalone Qt test of the real loader; never reads the production profile.
#include "../cpp/external_palette.cpp"
#include <QtCore/QCoreApplication>
#include <QtCore/QEventLoop>
#include <QtCore/QSaveFile>
#include <QtCore/QTemporaryDir>
#include <cstdlib>

static void require(bool value) { if (!value) std::abort(); }
static void settle()
{
    QEventLoop loop;
    QTimer::singleShot(250, &loop, &QEventLoop::quit);
    loop.exec();
}
int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    QTemporaryDir config;
    require(config.isValid());
    qputenv("XDG_CONFIG_HOME", config.path().toUtf8());
    auto *map = QQmlPropertyMap::create();
    new PaletteLoader(map);
    require(!map->value("valid").toBool());
    require(QDir().mkpath(config.path() + "/nectarlink"));
    const QString path = config.path() + "/nectarlink/noctalia-colors.json";
    QVariantMap colors;
    for (const char *role : {"primary", "onPrimary", "primaryContainer", "onPrimaryContainer",
             "secondaryContainer", "onSecondaryContainer", "surface", "surfaceContainerLow",
             "surfaceContainer", "surfaceContainerHigh", "surfaceContainerHighest",
             "onSurface", "onSurfaceVariant", "outline", "outlineVariant", "warning", "error", "onError"})
        colors.insert(QLatin1String(role), "#123456");
    auto write = [&](const QByteArray &bytes) {
        QSaveFile file(path);
        require(file.open(QIODevice::WriteOnly));
        require(file.write(bytes) == bytes.size() && file.commit());
        settle();
    };
    auto palette = [&](const QString &mode) {
        return QJsonDocument(QJsonObject{{"mode", mode}, {"colors", QJsonObject::fromVariantMap(colors)}}).toJson();
    };
    write(palette("dark"));
    require(map->value("valid").toBool() && map->value("dark").toBool());
    write(palette("light"));
    require(map->value("valid").toBool() && !map->value("dark").toBool());
    write("not json");
    require(!map->value("valid").toBool());
    colors["primary"] = "bad-color";
    write(palette("dark"));
    require(!map->value("valid").toBool());
    colors["primary"] = "#abcdef";
    write(palette("dark"));
    require(map->value("valid").toBool());
    require(QFile::remove(path));
    settle();
    require(!map->value("valid").toBool());
    require(!QFileInfo::exists(path + ".disabled"));
    delete map;
}
