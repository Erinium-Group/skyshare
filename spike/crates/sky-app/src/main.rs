// Pas de console en version publiée : la fenêtre et l'icône suffisent.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sky_app_lib::lancer();
}
