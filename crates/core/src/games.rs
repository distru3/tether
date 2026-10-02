//! Is this executable a game? Pure string heuristics shared by the agent's
//! auto-classifier (`games` category) and the session helper's HUD
//! suppression (never draw over a game; see the DWM flip notes there).
//!
//! These lists used to exist twice and had drifted apart: each side caught
//! games the other missed. Keep them here, and extend them here.

/// Game binaries recognised by name (lowercase).
const KNOWN_GAMES: &[&str] = &[
    "cs2.exe",
    "csgo.exe",
    "valorant.exe",
    "valorant-win64-shipping.exe",
    "vgc.exe",
    "dota2.exe",
    "leagueclient.exe",
    "leagueclientux.exe",
    "league of legends.exe",
    "gta5.exe",
    "gtav.exe",
    "rdr2.exe",
    "cyberpunk2077.exe",
    "witcher3.exe",
    "fortniteclient-win64-shipping.exe",
    "fortnite.exe",
    "overwatch.exe",
    "wow.exe",
    "wowclassic.exe",
    "diablo iv.exe",
    "rocketleague.exe",
    "apex.exe",
    "r5apex.exe",
    "pubg.exe",
    "tslgame.exe",
    "rainbowsix.exe",
    "rainbowsix_vulkan.exe",
    "destiny2.exe",
    "robloxplayerbeta.exe",
    "genshinimpact.exe",
    "starrail.exe",
    "zenlesszonezero.exe",
    "eldenring.exe",
    "sekiro.exe",
    "darksoulsiii.exe",
    "armoredcore6.exe",
    "helldivers2.exe",
    "baldursgate3.exe",
    "bg3.exe",
    "bg3_dx11.exe",
    "minecraft.exe",
    "halo-infinite.exe",
    "forzahorizon5.exe",
    "forzahorizon4.exe",
    "warframe.x64.exe",
];

/// Engine and packaging suffixes that almost only games use.
const GAME_SUFFIXES: &[&str] = &[
    "-shipping.exe",
    "_shipping.exe",
    "-win64-shipping.exe",
    "_win64-shipping.exe",
    "-win32-shipping.exe",
    "game.exe",
    "_launcher.exe",
];

/// Install-path fragments of game launchers and libraries (lowercase; both
/// separators where launchers use either).
const GAME_PATH_MARKERS: &[&str] = &[
    "\\steamapps\\common\\",
    "/steamapps/common/",
    "\\steamlibrary\\",
    "\\epic games\\",
    "\\gog galaxy\\games\\",
    "\\gog games\\",
    "\\riot games\\",
    "\\ubisoft\\",
    "\\ubisoft game launcher\\",
    "\\ea games\\",
    "\\electronic arts\\",
    "\\origin games\\",
    "\\xboxgames\\",
    "\\battle.net\\",
    "\\battlenet\\",
    "\\roblox\\versions\\",
    "\\.minecraft\\",
    "\\minecraft\\",
    "\\genshin impact\\",
    "\\honkai star rail\\",
    "\\zenless zone zero\\",
    "\\games\\",
    "\\game\\",
];

/// Whether an executable's file name (any case) looks like a game.
pub fn is_game_executable(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    KNOWN_GAMES.contains(&lower.as_str()) || GAME_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

/// Whether a full executable path (any case) sits in a game library.
pub fn is_game_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    GAME_PATH_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names_and_engine_suffixes_match_case_insensitively() {
        assert!(is_game_executable("CS2.EXE"));
        assert!(is_game_executable("Valorant-Win64-Shipping.exe"));
        assert!(is_game_executable("MyIndie_Game.exe"));
        assert!(is_game_executable("Something_Launcher.exe"));
        assert!(!is_game_executable("code.exe"));
        assert!(!is_game_executable("chrome.exe"));
    }

    #[test]
    fn launcher_libraries_from_either_former_list_match() {
        // Formerly only in the session helper's list.
        assert!(is_game_path(
            r"C:\Program Files (x86)\GOG Games\Witcher\witcher.exe"
        ));
        assert!(is_game_path(
            r"C:\Users\kid\AppData\Local\Roblox\Versions\v1\RobloxPlayer.exe"
        ));
        // Formerly only in the agent's classifier.
        assert!(is_game_path(
            r"D:\SteamLibrary\steamapps\common\Foo\foo.exe"
        ));
        assert!(is_game_path(r"C:\Program Files\Ubisoft\Anno\anno.exe"));
        assert!(!is_game_path(
            r"C:\Program Files\Microsoft Office\root\WINWORD.EXE"
        ));
    }
}
