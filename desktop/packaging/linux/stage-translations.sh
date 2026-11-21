#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Source this helper from the Linux packaging scripts.

stage_translations() {
    local bin_dir="$1" source_dir="$2" dest="$3"
    local lrelease="${LRELEASE:-}" qt_bins ts qm
    mkdir -p "${dest}"
    if [[ -z "${lrelease}" && -n "${QMAKE:-}" ]]; then
        qt_bins="$("${QMAKE}" -query QT_INSTALL_BINS || true)"
        if [[ -n "${qt_bins}" && -x "${qt_bins}/lrelease" ]]; then
            lrelease="${qt_bins}/lrelease"
        fi
    fi
    if [[ -z "${lrelease}" ]]; then
        lrelease="$(command -v lrelease-qt6 || command -v lrelease6 || command -v lrelease || true)"
    fi
    if [[ -z "${lrelease}" && -x /usr/lib/qt6/bin/lrelease ]]; then
        lrelease=/usr/lib/qt6/bin/lrelease
    fi

    for ts in "${source_dir}"/nectarlink_*.ts; do
        [[ -f "${ts}" ]] || continue
        qm="$(basename "${ts}" .ts).qm"
        if [[ -s "${bin_dir}/${qm}" && ! "${ts}" -nt "${bin_dir}/${qm}" ]]; then
            install -m 0644 "${bin_dir}/${qm}" "${dest}/${qm}"
        elif [[ -n "${lrelease}" ]]; then
            "${lrelease}" -silent "${ts}" -qm "${dest}/${qm}"
        else
            echo "error: ${qm} is missing or older than its source; set LRELEASE to Qt 6 lrelease" >&2
            return 1
        fi
        if [[ ! -s "${dest}/${qm}" ]]; then
            echo "error: translation was not staged: ${qm}" >&2
            return 1
        fi
    done
    if [[ ! -s "${dest}/nectarlink_ru.qm" ]]; then
        echo "error: Russian translation missing from the package" >&2
        return 1
    fi
}
