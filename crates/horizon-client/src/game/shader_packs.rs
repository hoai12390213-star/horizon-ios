//! Shader packs: the pack chosen in `config/iris.properties` (`enableShaders` and `shaderPack`, where
//! Iris keeps its choice, so a Java player finds it), set by `shader-pack` from the control socket or
//! the phone's launcher, and overridden for development by `HORIZON_SHADER_PACK=<name>`. It runs
//! `shaderpacks/<name>` (a folder or a zip beside `options.txt`) over the world. The programs of the
//! current dimension's folder are loaded when the dimension or the choice changes (spec 1.2).

use std::cell::RefCell;
use std::path::PathBuf;

use horizon_client::Result;
use horizon_client::camera::CameraFluid;
use horizon_client::gfx::pack_uniforms::PackInputs;
use horizon_shaders::{Macros, Pack, Properties, dimension_folder, load_post};
use serde_json::{Value, json};

use super::{Dim, Game};

pub(super) const HELP: &[&str] = &[
    "shader-pack [list|off|NAME]    the shader packs in shaderpacks/ and the one in use; choose one, or none",
];

thread_local! {
    /// The pack chosen in `config/iris.properties`, read once; `None` until then.
    static CHOSEN: RefCell<Option<Option<String>>> = const { RefCell::new(None) };
    /// The pack whose programs the renderer holds, and for which dimension.
    static LOADED: RefCell<Option<(String, i32)>> = const { RefCell::new(None) };
}

fn game_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_default()
}

fn settings_file() -> PathBuf {
    game_dir().join("config").join("iris.properties")
}

/// `shaderPack` while `enableShaders` is true, in a `key=value` properties file.
fn read_choice(text: &str) -> Option<String> {
    let mut pack = None;
    let mut enabled = true;
    for line in text.lines().map(str::trim) {
        if line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let Some((key, value)) = line.split_once(['=', ':']) else {
            continue;
        };
        match key.trim() {
            "shaderPack" => pack = Some(value.trim().to_owned()).filter(|v| !v.is_empty()),
            "enableShaders" => enabled = value.trim() == "true",
            _ => {}
        }
    }
    pack.filter(|_| enabled)
}

/// The pack to run: the environment's, else the chosen one.
fn chosen() -> Option<String> {
    if let Some(name) = std::env::var_os("HORIZON_SHADER_PACK") {
        return Some(name.to_string_lossy().into_owned());
    }
    CHOSEN.with_borrow_mut(|chosen| {
        chosen
            .get_or_insert_with(|| {
                read_choice(&std::fs::read_to_string(settings_file()).unwrap_or_default())
            })
            .clone()
    })
}

/// Keep `pack` (or none) as the choice, in the file and for the next frame. Other keys stay.
fn choose(pack: Option<&str>) -> std::io::Result<()> {
    let file = settings_file();
    let old = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<String> = old
        .lines()
        .filter(|line| {
            let key = line.split_once(['=', ':']).map(|(k, _)| k.trim());
            !matches!(key, Some("shaderPack" | "enableShaders"))
        })
        .map(str::to_owned)
        .collect();
    lines.push(format!("enableShaders={}", pack.is_some()));
    if let Some(pack) = pack {
        lines.push(format!("shaderPack={pack}"));
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&file, lines.join("\n") + "\n")?;
    CHOSEN.with_borrow_mut(|chosen| *chosen = Some(pack.map(str::to_owned)));
    Ok(())
}

/// The Shader Packs page's list and choice, read again (when it opens, and after a choice).
pub(super) fn fill_page(options: &mut horizon_client::options_ui::OptionsUi, makeup_enabled: bool) {
    options.shader_packs = horizon_shaders::discover(&game_dir().join("shaderpacks"))
        .into_iter()
        .map(|found| (found.name, found.problem))
        .collect();
    options.shader_pack = chosen();
    options.shader_selection = if makeup_enabled {
        horizon_client::options_ui::ShaderChoice::MakeUp
    } else {
        options
            .shader_pack
            .clone()
            .map_or(horizon_client::options_ui::ShaderChoice::Off, |name| {
                horizon_client::options_ui::ShaderChoice::Pack(name)
            })
    };
}

/// The page's choice: keep it, then show it.
pub(super) fn pick(
    options: &mut horizon_client::options_ui::OptionsUi,
    pack: Option<&str>,
    makeup_enabled: bool,
) {
    if let Err(e) = choose(pack) {
        log::warn!("Horizon: could not keep the shader pack choice: {e}");
    }
    fill_page(options, makeup_enabled);
}

/// The page's Open Shader Pack Folder.
pub(super) fn open_folder() {
    let dir = game_dir().join("shaderpacks");
    let _ = std::fs::create_dir_all(&dir);
    super::packs::open_folder(&dir);
}

/// `shader-pack`: list the packs, or choose one (`off` for none). None for other commands. Files only, so the
/// phone's launcher asks it before the game starts too (game/launcher.rs).
pub(super) fn shader_pack_command(words: &[&str], line: &str) -> Result<Option<Value>> {
    if words.first() != Some(&"shader-pack") {
        return Ok(None);
    }
    let packs = horizon_shaders::discover(&game_dir().join("shaderpacks"));
    // The name runs to the end of the line: pack names have spaces.
    let arg = line
        .trim_start()
        .strip_prefix("shader-pack")
        .map(str::trim)
        .unwrap_or("");
    match arg {
        "" | "list" => {}
        "off" => choose(None)?,
        name => {
            let Some(found) = packs.iter().find(|p| p.name == name) else {
                return Err(format!("no shader pack {name} in shaderpacks/").into());
            };
            if let Some(problem) = &found.problem {
                return Err(format!("shader pack {name} can't be used: {problem}").into());
            }
            choose(Some(name))?;
        }
    }
    let list: Vec<Value> = packs
        .iter()
        .map(|p| json!({ "name": p.name, "problem": p.problem }))
        .collect();
    Ok(Some(json!({ "packs": list, "selected": chosen() })))
}

impl Game<'_> {
    /// `shader-pack` from the control socket.
    pub(super) fn shader_pack_control(
        &mut self,
        words: &[&str],
        line: &str,
    ) -> Result<Option<Value>> {
        shader_pack_command(words, line)
    }

    /// Loads the chosen pack for the current dimension when either changed, and returns this frame's
    /// inputs to the pack's uniforms.
    pub(super) fn shader_pack_inputs(&mut self, camera: glam::DVec3, partial: f32) -> PackInputs {
        let Some(name) = chosen() else {
            // A pack that was turned off leaves the renderer.
            if LOADED.with_borrow_mut(Option::take).is_some()
                && let Some(renderer) = &mut self.renderer
                && let Some(dimension) = renderer.shader_pack_dimension()
            {
                renderer.set_shader_pack(dimension, None);
            }
            return PackInputs::default();
        };
        let dimension = match self.dim {
            Dim::Overworld => 0,
            Dim::Nether => -1,
            Dim::End => 1,
        };
        let current =
            LOADED.with_borrow(|loaded| loaded.as_ref() == Some(&(name.clone(), dimension)));
        if let Some(renderer) = &mut self.renderer
            && (!current || renderer.shader_pack_dimension() != Some(dimension))
        {
            LOADED.with_borrow_mut(|loaded| *loaded = Some((name.clone(), dimension)));
            let started = std::time::Instant::now();
            let path = std::env::current_dir()
                .unwrap_or_default()
                .join("shaderpacks")
                .join(&*name);
            match Pack::open(&path) {
                Ok(pack) => {
                    let macros = Macros::standard();
                    let text = pack.read_text("shaders.properties").unwrap_or_default();
                    let properties = Properties::parse(&text, &macros);
                    for note in &properties.notes {
                        log::warn!(
                            "shader pack {name}: shaders.properties:{}: {}",
                            note.line,
                            note.message
                        );
                    }
                    let folder = dimension_folder(&pack, dimension);
                    let post = load_post(&pack, &folder, &properties, &macros, None);
                    if let Some((programs, notes)) = renderer.set_shader_pack(dimension, Some(post))
                    {
                        for note in notes {
                            log::warn!("shader pack {name}: {note}");
                        }
                        log::info!(
                            "shader pack {name}: folder `{folder}`, {} programs ({}) in {} ms",
                            programs.len(),
                            programs.join(" "),
                            started.elapsed().as_millis()
                        );
                    }
                }
                Err(e) => {
                    log::warn!("shader pack {name}: {e}");
                    renderer.set_shader_pack(dimension, None);
                }
            }
        }
        let lerp = |(old, new): (f32, f32)| old + (new - old) * partial;
        let at = camera.floor().as_ivec3();
        let light = self.world.light(at.x, at.y, at.z);
        PackInputs {
            day_time: self.world.day_time,
            rain: lerp(self.world.weather.rain_level),
            thunder: lerp(self.world.weather.thunder_level),
            eye_in: match self.camera_fluid(camera.to_array()) {
                CameraFluid::None => 0,
                CameraFluid::Water => 1,
                CameraFluid::Lava => 2,
                CameraFluid::PowderSnow => 3,
            },
            blindness: f32::from(u8::from(self.potions.effects.get("blindness").is_some())),
            eye_light: [i32::from(light & 15), i32::from(light >> 4)],
            hide_gui: self.ui.hide_gui,
            brightness: self.settings.gamma,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_choice;

    #[test]
    fn the_choice_is_the_pack_while_shaders_are_on() {
        assert_eq!(
            read_choice("# Iris\nshaderPack=Complementary v5.zip\nenableShaders=true\n").as_deref(),
            Some("Complementary v5.zip")
        );
        assert_eq!(
            read_choice("shaderPack=BSL.zip\nenableShaders=false\n"),
            None
        );
        // Iris's default is on: a file with only the pack runs it.
        assert_eq!(
            read_choice("shaderPack = BSL.zip").as_deref(),
            Some("BSL.zip")
        );
        assert_eq!(read_choice("shaderPack=\nenableShaders=true"), None);
        assert_eq!(read_choice(""), None);
    }
}
