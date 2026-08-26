#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConflictPolicy {
    Skip,
    Overwrite,
    AutoRename,
}

impl ConflictPolicy {
    const ALL: &'static [ConflictPolicy] = &[
        ConflictPolicy::Skip,
        ConflictPolicy::Overwrite,
        ConflictPolicy::AutoRename,
    ];

    fn label(self) -> &'static str {
        match self {
            ConflictPolicy::Skip => "Skip",
            ConflictPolicy::Overwrite => "Overwrite",
            ConflictPolicy::AutoRename => "Auto-rename",
        }
    }
}

/// Given a target path that already exists, produce a non-existing alternative.
fn unique_path(target: &Path) -> PathBuf {
    let stem = target.file_stem().unwrap_or_default().to_string_lossy();
    let ext = target.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let parent = target.parent().unwrap_or(Path::new("."));
    for i in 1..u32::MAX {
        let candidate = parent.join(format!("{stem}({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    target.to_path_buf() // fallback, should never happen
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 600.0])
            .with_min_inner_size([600.0, 480.0])
            .with_title("Copy with Parent Levels"),
        ..Default::default()
    };
    eframe::run_native(
        "Copy with Parent Levels",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

// ── App state ──────────────────────────────────────────────────────────

struct App {
    files: Vec<PathBuf>,
    parent_levels: u32,
    destination: Option<PathBuf>,
    conflict: ConflictPolicy,
    status: String,
    pending_files: Arc<Mutex<Option<Vec<PathBuf>>>>,
    pending_dest: Arc<Mutex<Option<PathBuf>>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            parent_levels: 2,
            destination: None,
            conflict: ConflictPolicy::Skip,
            status: "Ready — select files to begin.".into(),
            pending_files: Arc::new(Mutex::new(None)),
            pending_dest: Arc::new(Mutex::new(None)),
        }
    }
}

// ── Core logic ─────────────────────────────────────────────────────────

fn relative_dest(filepath: &Path, parent_levels: u32) -> PathBuf {
    let resolved = filepath.canonicalize().unwrap_or_else(|_| filepath.to_path_buf());
    let mut anchor = resolved.parent().unwrap_or(&resolved).to_path_buf();
    for _ in 0..parent_levels {
        anchor = anchor.parent().unwrap_or(&anchor).to_path_buf();
    }
    resolved
        .strip_prefix(&anchor)
        .unwrap_or(&resolved)
        .to_path_buf()
}

/// Recursively collect all files under a directory.
fn walk_dir(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk_dir(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}

fn copy_files(
    files: &[PathBuf],
    parent_levels: u32,
    dest: &Path,
    conflict: ConflictPolicy,
) -> String {
    let mut copied = 0u32;
    let mut skipped = 0u32;
    let mut overwritten = 0u32;
    let mut renamed = 0u32;
    let mut errors: Vec<String> = Vec::new();

    for file in files {
        let rel = relative_dest(file, parent_levels);
        let mut target = dest.join(&rel);
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                errors.push(format!("{}: {e}", file.display()));
                continue;
            }
        }

        if target.exists() {
            match conflict {
                ConflictPolicy::Skip => {
                    skipped += 1;
                    continue;
                }
                ConflictPolicy::Overwrite => {
                    overwritten += 1;
                }
                ConflictPolicy::AutoRename => {
                    target = unique_path(&target);
                    renamed += 1;
                }
            }
        }

        match std::fs::copy(file, &target) {
            Ok(_) => copied += 1,
            Err(e) => errors.push(format!("{}: {e}", file.display())),
        }
    }

    let mut msg = format!("Copied {copied} file(s).");
    if skipped > 0 {
        msg.push_str(&format!("  Skipped {skipped} (already exist)."));
    }
    if overwritten > 0 {
        msg.push_str(&format!("  Overwrote {overwritten}."));
    }
    if renamed > 0 {
        msg.push_str(&format!("  Renamed {renamed} (avoided conflicts)."));
    }
    if !errors.is_empty() {
        msg.push_str(&format!("  {} error(s).", errors.len()));
        for e in &errors {
            msg.push_str(&format!("\n  \u{2022} {e}"));
        }
    }
    msg
}

// ── Tree structure ─────────────────────────────────────────────────────

enum TreeNode {
    Dir(BTreeMap<String, TreeNode>),
    File,
}

/// Insert a relative path into the tree.
fn tree_insert(node: &mut BTreeMap<String, TreeNode>, components: &[String]) {
    if let Some((name, rest)) = components.split_first() {
        if rest.is_empty() {
            // This is a file
            node.insert(name.clone(), TreeNode::File);
        } else {
            // This is a directory — recurse
            let child = node
                .entry(name.clone())
                .or_insert_with(|| TreeNode::Dir(BTreeMap::new()));
            if let TreeNode::Dir(children) = child {
                tree_insert(children, rest);
            }
        }
    }
}

fn build_tree(paths: &[PathBuf]) -> BTreeMap<String, TreeNode> {
    let mut root = BTreeMap::new();
    for rel in paths {
        let components: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if !components.is_empty() {
            tree_insert(&mut root, &components);
        }
    }
    root
}

fn render_tree(ui: &mut egui::Ui, tree: &BTreeMap<String, TreeNode>, depth: u32) {
    for (name, node) in tree {
        match node {
            TreeNode::Dir(children) => {
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 18.0);
                    ui.strong("📁 ");
                    ui.strong(name);
                });
                if !children.is_empty() {
                    render_tree(ui, children, depth + 1);
                }
            }
            TreeNode::File => {
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 18.0);
                    ui.label("📄 ");
                    ui.label(name.as_str());
                });
            }
        }
    }
}

// ── GUI ────────────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Collect results from native file dialogs
        if let Some(files) = self.pending_files.lock().unwrap().take() {
            self.files.extend(files);
            self.status = format!("{} file(s) selected.", self.files.len());
        }
        if let Some(dest) = self.pending_dest.lock().unwrap().take() {
            self.destination = Some(dest);
        }

        // Handle drag-and-drop
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            let mut added = 0u32;
            for df in &dropped {
                if let Some(path) = &df.path {
                    if path.is_dir() {
                        let files = walk_dir(path);
                        added += files.len() as u32;
                        self.files.extend(files);
                    } else {
                        added += 1;
                        self.files.push(path.clone());
                    }
                }
            }
            self.status = format!("Added {added} file(s) via drop. {} total.", self.files.len());
        }

        // Bottom status bar
        let zoom = ctx.zoom_factor();
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{:.0}%", zoom * 100.0));
                });
            });
        });

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Copy with Parent Levels");
            ui.separator();

            // Source files
            let is_dragging = !ctx.input(|i| i.raw.hovered_files.is_empty());
            let frame_color = if is_dragging {
                egui::Color32::from_rgb(60, 120, 200)
            } else {
                ui.visuals().widgets.noninteractive.bg_fill
            };

            egui::Frame::NONE
                .fill(frame_color)
                .corner_radius(8.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                ui.label(if is_dragging {
                    "Drop files here"
                } else {
                    "Source files"
                });
                ui.horizontal(|ui| {
                    if ui.button("Select files…").clicked() {
                        let pending = self.pending_files.clone();
                        let ctx = ctx.clone();
                        std::thread::spawn(move || {
                            let result = rfd::FileDialog::new()
                                .set_title("Select files to copy")
                                .pick_files();
                            *pending.lock().unwrap() = result;
                            ctx.request_repaint();
                        });
                    }
                    if ui.button("Clear").clicked() {
                        self.files.clear();
                        self.status = "File list cleared.".into();
                    }
                });

                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    if self.files.is_empty() {
                        ui.label("(no files selected — drag & drop or use the button above)");
                    } else {
                        for (i, f) in self.files.iter().enumerate() {
                            ui.label(format!("{}.  {}", i + 1, f.display()));
                        }
                    }
                });
            });

            ui.add_space(6.0);

            // Parent levels + Conflict policy + Destination
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    ui.label("Parent levels");
                    ui.add(egui::DragValue::new(&mut self.parent_levels).range(0..=20));
                });

                ui.group(|ui| {
                    ui.label("On conflict");
                    egui::ComboBox::from_id_salt("conflict_policy")
                        .selected_text(self.conflict.label())
                        .show_ui(ui, |ui| {
                            for &policy in ConflictPolicy::ALL {
                                ui.selectable_value(&mut self.conflict, policy, policy.label());
                            }
                        });
                });

                ui.group(|ui| {
                    ui.label("Destination");
                    ui.horizontal(|ui| {
                        if ui.button("Select…").clicked() {
                            let pending = self.pending_dest.clone();
                            let ctx = ctx.clone();
                            std::thread::spawn(move || {
                                let result = rfd::FileDialog::new()
                                    .set_title("Select destination directory")
                                    .pick_folder();
                                *pending.lock().unwrap() = result;
                                ctx.request_repaint();
                            });
                        }
                        match &self.destination {
                            Some(d) => ui.label(d.display().to_string()),
                            None => ui.label("(not selected)"),
                        }
                    });
                });
            });

            ui.add_space(6.0);

            // Tree preview
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong("Destination tree preview");
                    if let Some(dest) = &self.destination {
                        ui.label(
                            egui::RichText::new(dest.display().to_string())
                                .small()
                                .weak(),
                        );
                    }
                });
                ui.separator();

                if self.files.is_empty() {
                    ui.label(
                        egui::RichText::new("Select files to see the preview tree.").weak(),
                    );
                } else {
                    let rels: Vec<PathBuf> = self
                        .files
                        .iter()
                        .map(|f| relative_dest(f, self.parent_levels))
                        .collect();
                    let tree = build_tree(&rels);
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        render_tree(ui, &tree, 0);
                    });
                }
            });

            ui.add_space(8.0);

            // Copy button
            ui.horizontal(|ui| {
                let can_copy = !self.files.is_empty() && self.destination.is_some();
                let btn = ui.add_enabled(can_copy, egui::Button::new("Copy files"));
                if btn.clicked() {
                    if let Some(dest) = &self.destination {
                        self.status = copy_files(&self.files, self.parent_levels, dest, self.conflict);
                    }
                }
                if !can_copy {
                    ui.label(
                        egui::RichText::new("(select files and destination first)")
                            .small()
                            .weak(),
                    );
                }
            });
        });
    }
}
