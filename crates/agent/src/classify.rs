//! Curated signature database for auto-classifying apps.
//!
//! Matches on the lower-cased `AppKey::basename` — the executable file name on
//! Windows, the desktop-entry id on Linux. Returns a primary category plus
//! optional tags, as slugs; the caller resolves them to ids.
//!
//! This is deliberately a small, hand-curated table for M1: enough to make the
//! dashboard meaningful on a real machine without pretending to be exhaustive.
//! A packaged signature database ships in a later milestone. Browsers are left
//! uncategorized on purpose — a browser is whatever is in front of it, and
//! site-level classification arrives with the DNS filter.
//!
//! Classifier output must never overwrite a human decision: the caller only
//! applies a result when the app has not been `user_classified`.

use st_core::model::AppKey;

/// A matched classification, as category slugs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub primary: &'static str,
    pub tags: &'static [&'static str],
}

/// `(basename, primary category, additional tag categories)`.
///
/// Keys are lower-case. Windows paths are already lower-cased by
/// `AppKey::windows_exe`; Linux names are lower-cased at match time so the
/// table stays case-insensitive.
const SIGNATURES: &[(&str, &str, &[&str])] = &[
    // Games.
    ("steam.exe", "games", &[]),
    ("steamwebhelper.exe", "games", &[]),
    ("epicgameslauncher.exe", "games", &[]),
    ("epicwebhelper.exe", "games", &[]),
    ("goggalaxy.exe", "games", &[]),
    ("battle.net.exe", "games", &[]),
    ("uplay.exe", "games", &[]),
    ("ubisoftconnect.exe", "games", &[]),
    ("origin.exe", "games", &[]),
    ("ea.exe", "games", &[]),
    ("leagueclient.exe", "games", &[]),
    ("league of legends.exe", "games", &[]),
    ("valorant.exe", "games", &[]),
    ("valorant-win64-shipping.exe", "games", &[]),
    ("cs2.exe", "games", &[]),
    ("csgo.exe", "games", &[]),
    ("rocketleague.exe", "games", &[]),
    ("fortniteclient-win64-shipping.exe", "games", &[]),
    ("gta5.exe", "games", &[]),
    ("minecraft.exe", "games", &[]),
    ("javaw.exe", "games", &[]),
    ("terraria.exe", "games", &[]),
    ("stardew valley.exe", "games", &[]),
    ("factorio.exe", "games", &[]),
    ("rimworld.exe", "games", &[]),
    ("civ6.exe", "games", &[]),
    ("diablo iv.exe", "games", &[]),
    ("wow.exe", "games", &[]),
    ("wowclassic.exe", "games", &[]),
    ("osu!.exe", "games", &[]),
    ("robloxplayerbeta.exe", "games", &[]),
    ("roblox.exe", "games", &[]),
    ("mudlet.exe", "games", &[]),
    // Short-form video overlaps social media by design.
    ("tiktok.exe", "short-form-video", &["social-media"]),
    // Video streaming.
    ("netflix.exe", "video-streaming", &[]),
    ("twitch.exe", "video-streaming", &[]),
    ("plex.exe", "video-streaming", &[]),
    ("vlc.exe", "video-streaming", &[]),
    ("mpc-hc.exe", "video-streaming", &[]),
    ("kodi.exe", "video-streaming", &[]),
    ("hulu.exe", "video-streaming", &[]),
    ("disneyplus.exe", "video-streaming", &[]),
    // Music & audio.
    ("spotify.exe", "music-audio", &[]),
    ("itunes.exe", "music-audio", &[]),
    ("musicbee.exe", "music-audio", &[]),
    ("foobar2000.exe", "music-audio", &[]),
    ("winamp.exe", "music-audio", &[]),
    ("tidal.exe", "music-audio", &[]),
    ("apple music.exe", "music-audio", &[]),
    // Communication.
    ("discord.exe", "communication", &[]),
    ("telegram.exe", "communication", &[]),
    ("whatsapp.exe", "communication", &[]),
    ("ms-teams.exe", "communication", &[]),
    ("teams.exe", "communication", &[]),
    ("zoom.exe", "communication", &[]),
    ("slack.exe", "communication", &[]),
    ("signal.exe", "communication", &[]),
    ("skype.exe", "communication", &[]),
    ("wechat.exe", "communication", &[]),
    ("line.exe", "communication", &[]),
    ("element.exe", "communication", &[]),
    ("thunderbird.exe", "communication", &[]),
    ("outlook.exe", "communication", &[]),
    ("mail.exe", "communication", &[]),
    ("messenger.exe", "communication", &[]),
    // Productivity & office.
    ("winword.exe", "productivity", &[]),
    ("excel.exe", "productivity", &[]),
    ("powerpnt.exe", "productivity", &[]),
    ("onenote.exe", "productivity", &[]),
    ("notion.exe", "productivity", &[]),
    ("obsidian.exe", "productivity", &[]),
    ("evernote.exe", "productivity", &[]),
    ("trello.exe", "productivity", &[]),
    ("todoist.exe", "productivity", &[]),
    ("soffice.exe", "productivity", &[]),
    ("swriter.exe", "productivity", &[]),
    ("scalc.exe", "productivity", &[]),
    ("wps.exe", "productivity", &[]),
    // Creativity & design.
    ("photoshop.exe", "creativity", &[]),
    ("illustrator.exe", "creativity", &[]),
    ("premiere pro.exe", "creativity", &[]),
    ("afterfx.exe", "creativity", &[]),
    ("resolve.exe", "creativity", &[]),
    ("figma.exe", "creativity", &[]),
    ("gimp.exe", "creativity", &[]),
    ("inkscape.exe", "creativity", &[]),
    ("krita.exe", "creativity", &[]),
    ("blender.exe", "creativity", &[]),
    ("mayacmd.exe", "creativity", &[]),
    ("affinity photo.exe", "creativity", &[]),
    ("audacity.exe", "creativity", &[]),
    ("obs.exe", "creativity", &[]),
    ("paint.net.exe", "creativity", &[]),
    ("mspaint.exe", "creativity", &[]),
    // Education & reading.
    ("kindle.exe", "education", &[]),
    ("calibre.exe", "education", &[]),
    ("acrord32.exe", "education", &[]),
    ("acrobat.exe", "education", &[]),
    ("anki.exe", "education", &[]),
    // Finance.
    ("quicken.exe", "finance", &[]),
    ("turbotax.exe", "finance", &[]),
    ("tradingview.exe", "finance", &[]),
    ("binance.exe", "finance", &[]),
    // AI assistants.
    ("chatgpt.exe", "ai-chatbots", &[]),
    ("claude.exe", "ai-chatbots", &[]),
    // Development (never blockable).
    ("code.exe", "development", &[]),
    ("cursor.exe", "development", &[]),
    ("devenv.exe", "development", &[]),
    ("idea64.exe", "development", &[]),
    ("pycharm64.exe", "development", &[]),
    ("webstorm64.exe", "development", &[]),
    ("rider64.exe", "development", &[]),
    ("clion64.exe", "development", &[]),
    ("goland64.exe", "development", &[]),
    ("rustrover64.exe", "development", &[]),
    ("datagrip64.exe", "development", &[]),
    ("eclipse.exe", "development", &[]),
    ("notepad++.exe", "development", &[]),
    ("sublime_text.exe", "development", &[]),
    ("gvim.exe", "development", &[]),
    ("emacs.exe", "development", &[]),
    ("git-bash.exe", "development", &[]),
    ("wt.exe", "development", &[]),
    ("windows terminal.exe", "development", &[]),
    ("cmd.exe", "development", &[]),
    ("powershell.exe", "development", &[]),
    ("powershell_ise.exe", "development", &[]),
    ("docker desktop.exe", "development", &[]),
    ("postman.exe", "development", &[]),
    ("insomnia.exe", "development", &[]),
    ("putty.exe", "development", &[]),
    ("studio64.exe", "development", &[]),
    ("unity.exe", "development", &[]),
    ("unrealeditor.exe", "development", &[]),
    ("godot.exe", "development", &[]),
    ("github desktop.exe", "development", &[]),
    ("sourcetree.exe", "development", &[]),
    ("dbeaver.exe", "development", &[]),
    ("heidisql.exe", "development", &[]),
    ("octave.exe", "development", &[]),
    ("matlab.exe", "development", &[]),
    ("wsl.exe", "development", &[]),
    ("mintty.exe", "development", &[]),
    // Utilities & system (never blockable).
    ("explorer.exe", "utilities-system", &[]),
    ("taskmgr.exe", "utilities-system", &[]),
    ("regedit.exe", "utilities-system", &[]),
    ("notepad.exe", "utilities-system", &[]),
    ("calculator.exe", "utilities-system", &[]),
    ("systemsettings.exe", "utilities-system", &[]),
    ("control.exe", "utilities-system", &[]),
    ("onedrive.exe", "utilities-system", &[]),
    ("dropbox.exe", "utilities-system", &[]),
    ("7zfm.exe", "utilities-system", &[]),
    ("winrar.exe", "utilities-system", &[]),
    ("qbittorrent.exe", "utilities-system", &[]),
    ("snippingtool.exe", "utilities-system", &[]),
    ("powertoy.exe", "utilities-system", &[]),
    ("vmware.exe", "utilities-system", &[]),
    ("virtualbox.exe", "utilities-system", &[]),
    ("ccleaner.exe", "utilities-system", &[]),
];

/// Classify an app by its key. Returns `None` for anything the table does not
/// recognise, in which case the caller keeps the `uncategorized` default.
pub fn classify(key: &AppKey) -> Option<Classification> {
    let basename = key.basename().to_lowercase();
    SIGNATURES
        .iter()
        .find(|(name, _, _)| *name == basename)
        .map(|(_, primary, tags)| Classification { primary, tags })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(path: &str) -> AppKey {
        AppKey::windows_exe(path)
    }

    #[test]
    fn matches_windows_exe_by_basename() {
        let c = classify(&exe("C:\\Program Files\\Steam\\steam.exe")).expect("steam");
        assert_eq!(c.primary, "games");
        assert!(c.tags.is_empty());
    }

    #[test]
    fn matching_is_case_insensitive() {
        let c =
            classify(&exe("C:\\Users\\me\\AppData\\Local\\Discord\\Discord.EXE")).expect("discord");
        assert_eq!(c.primary, "communication");
    }

    #[test]
    fn short_form_video_also_carries_social_media_tag() {
        let c = classify(&exe("C:\\tiktok.exe")).expect("tiktok");
        assert_eq!(c.primary, "short-form-video");
        assert_eq!(c.tags, &["social-media"]);
    }

    #[test]
    fn development_tools_classify_but_are_never_blockable() {
        let c = classify(&exe(
            "C:\\Users\\me\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe",
        ))
        .expect("code");
        assert_eq!(c.primary, "development");
    }

    #[test]
    fn unknown_apps_return_none() {
        assert!(classify(&exe("C:\\random\\someapp.exe")).is_none());
    }

    #[test]
    fn aumid_basename_matches_when_the_suffix_is_known() {
        // `basename()` on an AUMID returns the whole id; anything not in the
        // table stays uncategorized until packaged-app ids are curated.
        let key = AppKey::WindowsAumid("Microsoft.WindowsTerminal_8wekyb3d8bbwe".into());
        assert!(classify(&key).is_none());
    }

    #[test]
    fn every_signature_primary_exists_in_the_builtin_taxonomy() {
        for (_, primary, tags) in SIGNATURES {
            assert!(
                st_core::category::builtin_by_slug(primary).is_some(),
                "unknown primary slug: {primary}"
            );
            for tag in *tags {
                assert!(
                    st_core::category::builtin_by_slug(tag).is_some(),
                    "unknown tag slug: {tag}"
                );
            }
        }
    }

    #[test]
    fn signatures_do_not_reference_filter_only_categories() {
        // adult-content and gambling are site filters; apps must never land there.
        for (_, primary, tags) in SIGNATURES {
            assert_ne!(*primary, "adult-content");
            assert_ne!(*primary, "gambling");
            for tag in *tags {
                assert_ne!(*tag, "adult-content");
                assert_ne!(*tag, "gambling");
            }
        }
    }

    #[test]
    fn signatures_do_not_duplicate_a_key() {
        let mut seen = std::collections::HashSet::new();
        for (name, _, _) in SIGNATURES {
            assert!(seen.insert(*name), "duplicate signature: {name}");
        }
    }
}
