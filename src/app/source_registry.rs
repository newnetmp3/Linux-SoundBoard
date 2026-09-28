#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSupport {
    Working,
    Candidate,
}

#[derive(Debug, Clone, Copy)]
pub struct SourceCandidate {
    pub id: u16,
    pub name: &'static str,
    pub url: &'static str,
    pub support: SourceSupport,
}

pub const SOURCES: &[SourceCandidate] = &[
    SourceCandidate { id: 1, name: "Myinstants", url: "https://www.myinstants.com/", support: SourceSupport::Working },
    SourceCandidate { id: 2, name: "Voicy", url: "https://www.voicy.network/meme-soundboard", support: SourceSupport::Candidate },
    SourceCandidate { id: 3, name: "Voicemod Tuna", url: "https://tuna.voicemod.net/", support: SourceSupport::Candidate },
    SourceCandidate { id: 4, name: "Soundboards.gg", url: "https://soundboards.gg/", support: SourceSupport::Candidate },
    SourceCandidate { id: 5, name: "101soundboards", url: "https://www.101soundboards.com/featured/popular", support: SourceSupport::Candidate },
    SourceCandidate { id: 6, name: "Soundboard.com", url: "https://www.soundboard.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 7, name: "BoardSounds", url: "https://boardsounds.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 8, name: "TapSounds", url: "https://tapsounds.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 9, name: "Sound-Buttons.com", url: "https://www.sound-buttons.com/", support: SourceSupport::Working },
    SourceCandidate { id: 10, name: "SoundButtons.io", url: "https://soundbuttons.io/", support: SourceSupport::Candidate },
    SourceCandidate { id: 11, name: "SoundButtons.net", url: "https://soundbuttons.net/", support: SourceSupport::Candidate },
    SourceCandidate { id: 12, name: "SoundButtonsWorld", url: "https://soundbuttonsworld.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 13, name: "Meme Instants", url: "https://memeinstants.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 14, name: "Realm of Darkness", url: "https://www.realmofdarkness.net/sb/", support: SourceSupport::Candidate },
    SourceCandidate { id: 15, name: "WavSource", url: "https://www.wavsource.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 16, name: "Movie Sound Clips", url: "https://www.moviesoundclips.net/", support: SourceSupport::Working },
    SourceCandidate { id: 17, name: "Zedge", url: "https://www.zedge.net/", support: SourceSupport::Candidate },
    SourceCandidate { id: 18, name: "Myinstants.net", url: "https://myinstants.net/", support: SourceSupport::Candidate },
    SourceCandidate { id: 19, name: "Myinstants.app", url: "https://myinstants.app/memes-soundboard", support: SourceSupport::Candidate },
    SourceCandidate { id: 20, name: "My-Instants.com", url: "https://my-instants.com/", support: SourceSupport::Working },
    SourceCandidate { id: 21, name: "Freesound", url: "https://freesound.org/", support: SourceSupport::Working },
    SourceCandidate { id: 22, name: "Pixabay Sound Effects", url: "https://pixabay.com/sound-effects/", support: SourceSupport::Candidate },
    SourceCandidate { id: 23, name: "Mixkit", url: "https://mixkit.co/free-sound-effects/", support: SourceSupport::Candidate },
    SourceCandidate { id: 24, name: "Zapsplat", url: "https://www.zapsplat.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 25, name: "SoundBible", url: "https://soundbible.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 26, name: "FreeSFX", url: "https://www.freesfx.co.uk/", support: SourceSupport::Candidate },
    SourceCandidate { id: 27, name: "FreeSoundEffects.com", url: "https://www.freesoundeffects.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 28, name: "Orange Free Sounds", url: "https://orangefreesounds.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 29, name: "SoundJay", url: "https://www.soundjay.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 30, name: "SoundGator", url: "https://www.soundgator.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 31, name: "SoundDino", url: "https://sounddino.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 32, name: "SoundEffects+", url: "https://www.soundeffectsplus.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 33, name: "Free Sounds Library", url: "https://www.freesoundslibrary.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 34, name: "SFX Library", url: "https://www.sfxlibrary.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 35, name: "BigSoundBank", url: "https://bigsoundbank.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 36, name: "Salamisound", url: "https://www.salamisound.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 37, name: "Sound-Fishing", url: "https://www.soundfishing.eu/", support: SourceSupport::Candidate },
    SourceCandidate { id: 38, name: "PacDV", url: "https://www.pacdv.com/sounds/", support: SourceSupport::Candidate },
    SourceCandidate { id: 39, name: "Partners In Rhyme", url: "https://www.partnersinrhyme.com/pir/PIRsfx.shtml", support: SourceSupport::Candidate },
    SourceCandidate { id: 40, name: "Fesliyan Studios", url: "https://www.fesliyanstudios.com/royalty-free-sound-effects-download", support: SourceSupport::Candidate },
    SourceCandidate { id: 41, name: "BBC Sound Effects", url: "https://sound-effects.bbcrewind.co.uk/", support: SourceSupport::Candidate },
    SourceCandidate { id: 42, name: "99Sounds", url: "https://99sounds.org/", support: SourceSupport::Candidate },
    SourceCandidate { id: 43, name: "Free to Use Sounds", url: "https://www.freetousesounds.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 44, name: "Soundimage", url: "https://soundimage.org/", support: SourceSupport::Candidate },
    SourceCandidate { id: 45, name: "Uppbeat", url: "https://uppbeat.io/sfx", support: SourceSupport::Candidate },
    SourceCandidate { id: 46, name: "SoundsCrate", url: "https://soundscrate.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 47, name: "Sound Effect Lab", url: "https://soundeffect-lab.info/", support: SourceSupport::Candidate },
    SourceCandidate { id: 48, name: "OtoLogic", url: "https://otologic.jp/", support: SourceSupport::Candidate },
    SourceCandidate { id: 49, name: "TK's Free Sound FX", url: "https://taira-komori.net/freesounden.html", support: SourceSupport::Candidate },
    SourceCandidate { id: 50, name: "Maou Audio", url: "https://maou.audio/", support: SourceSupport::Candidate },
    SourceCandidate { id: 51, name: "Soundsnap", url: "https://www.soundsnap.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 52, name: "Sonniss", url: "https://sonniss.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 53, name: "A Sound Effect", url: "https://www.asoundeffect.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 54, name: "BOOM Library", url: "https://www.boomlibrary.com/shop/", support: SourceSupport::Candidate },
    SourceCandidate { id: 55, name: "Pro Sound Effects", url: "https://www.prosoundeffects.com/collections/all", support: SourceSupport::Candidate },
    SourceCandidate { id: 56, name: "Sound Ideas", url: "https://sound-ideas.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 57, name: "SoundDogs", url: "https://sounddogs.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 58, name: "Soundrangers", url: "https://www.soundrangers.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 59, name: "Storyblocks", url: "https://www.storyblocks.com/audio/search?media-type=sound-effects", support: SourceSupport::Candidate },
    SourceCandidate { id: 60, name: "Epidemic Sound", url: "https://www.epidemicsound.com/sound-effects/", support: SourceSupport::Candidate },
    SourceCandidate { id: 61, name: "Artlist", url: "https://artlist.io/sfx", support: SourceSupport::Candidate },
    SourceCandidate { id: 62, name: "Envato Elements", url: "https://elements.envato.com/sound-effects", support: SourceSupport::Candidate },
    SourceCandidate { id: 63, name: "AudioJungle", url: "https://audiojungle.net/category/sound", support: SourceSupport::Candidate },
    SourceCandidate { id: 64, name: "Motion Array", url: "https://motionarray.com/sound-effects/", support: SourceSupport::Candidate },
    SourceCandidate { id: 65, name: "Pond5", url: "https://www.pond5.com/sound-effects/", support: SourceSupport::Candidate },
    SourceCandidate { id: 66, name: "HookSounds", url: "https://www.hooksounds.com/sound-effects/", support: SourceSupport::Candidate },
    SourceCandidate { id: 67, name: "TunePocket", url: "https://www.tunepocket.com/sound-effects/", support: SourceSupport::Candidate },
    SourceCandidate { id: 68, name: "Magnific formerly Videvo", url: "https://www.magnific.com/audio/sound-effects", support: SourceSupport::Candidate },
    SourceCandidate { id: 69, name: "OpenGameArt", url: "https://opengameart.org/", support: SourceSupport::Working },
    SourceCandidate { id: 70, name: "Kenney", url: "https://kenney.nl/assets", support: SourceSupport::Working },
    SourceCandidate { id: 71, name: "itch.io", url: "https://itch.io/game-assets/tag-sound-effects", support: SourceSupport::Candidate },
    SourceCandidate { id: 72, name: "The Sounds Resource", url: "https://www.sounds-resource.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 73, name: "GameDev Market", url: "https://www.gamedevmarket.net/category/audio/sound-fx", support: SourceSupport::Candidate },
    SourceCandidate { id: 74, name: "Unity Asset Store", url: "https://assetstore.unity.com/audio", support: SourceSupport::Candidate },
    SourceCandidate { id: 75, name: "Fab", url: "https://www.fab.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 76, name: "GameMaster Audio", url: "https://www.gamemasteraudio.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 77, name: "WOW Sound", url: "https://wowsound.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 78, name: "Ovani Sound", url: "https://ovanisound.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 79, name: "We Love Indies", url: "https://www.weloveindies.com/en", support: SourceSupport::Candidate },
    SourceCandidate { id: 80, name: "Epic Stock Media", url: "https://epicstockmedia.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 81, name: "David Dumais Audio", url: "https://www.daviddumaisaudio.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 82, name: "Ghosthack", url: "https://www.ghosthack.de/", support: SourceSupport::Candidate },
    SourceCandidate { id: 83, name: "Bluezone Corporation", url: "https://www.bluezone-corporation.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 84, name: "SoundMorph", url: "https://soundmorph.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 85, name: "Krotos", url: "https://www.krotosaudio.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 86, name: "Soundly", url: "https://getsoundly.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 87, name: "HISSandaROAR", url: "https://hissandaroar.com/v3/", support: SourceSupport::Candidate },
    SourceCandidate { id: 88, name: "The Recordist", url: "https://therecordist.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 89, name: "TONSTURM", url: "https://tonsturm.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 90, name: "344 SFX", url: "https://www.344sfx.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 91, name: "Blastwave FX", url: "https://blastwavefx.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 92, name: "Hollywood Edge", url: "https://www.hollywoodedge.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 93, name: "Sonic Salute", url: "https://sonicsalute.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 94, name: "ShapingWaves", url: "https://www.shapingwaves.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 95, name: "SoundBits", url: "https://soundbits.de/", support: SourceSupport::Candidate },
    SourceCandidate { id: 96, name: "PMSFX", url: "https://www.pmsfx.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 97, name: "Glitchedtones", url: "https://glitchedtones.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 98, name: "Sample Focus", url: "https://samplefocus.com/", support: SourceSupport::Candidate },
    SourceCandidate { id: 99, name: "Tabletop Audio", url: "https://tabletopaudio.com/", support: SourceSupport::Working },
    SourceCandidate { id: 100, name: "Ambient Mixer", url: "https://www.ambient-mixer.com/", support: SourceSupport::Candidate },
];

pub fn working_sources() -> impl Iterator<Item = &'static SourceCandidate> {
    SOURCES
        .iter()
        .filter(|source| source.support == SourceSupport::Working)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_registry_keeps_all_one_hundred_sources() {
        assert_eq!(SOURCES.len(), 100);
        assert_eq!(SOURCES.first().map(|source| source.id), Some(1));
        assert_eq!(SOURCES.last().map(|source| source.id), Some(100));
    }

    #[test]
    fn source_ids_are_unique_and_contiguous() {
        for (index, source) in SOURCES.iter().enumerate() {
            assert_eq!(source.id as usize, index + 1);
        }
    }
}
