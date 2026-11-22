use super::*;

#[test]
fn external_triangle_validates_indices_paths_hierarchy_and_materials() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/m9-independent-triangle");
    let root = base.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let mut bytes: Vec<_> = [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    bytes.extend_from_slice(&[0, 1, 2]);
    std::fs::write(root.join("mesh body.bin"), &bytes).unwrap();
    std::fs::write(base.join("outside.bin"), &bytes).unwrap();
    // The fixture uses unnormalized glTF coordinates and a percent-encoded URI.
    let model = r#"{
        "asset":{"version":"2.0"},
        "scene":0,"scenes":[{"nodes":[0]}],
        "nodes":[{"translation":[2,0,0],"children":[1]},{"name":"socket","translation":[0,3,0],"mesh":0}],
        "buffers":[{"byteLength":39,"uri":"mesh%20body.bin"}],
        "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":3}],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5121,"count":3,"type":"SCALAR"}
        ],
        "materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.5,0.25,0.75,1]}}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}]
    }"#;
    std::fs::write(root.join("triangle.gltf"), model).unwrap();
    let mut library = AssetLibrary::New(&root).unwrap();
    let mesh = library.Load(Path::new("triangle.gltf")).unwrap();
    let again = library.Load(Path::new("./triangle.gltf")).unwrap();
    assert!(Arc::ptr_eq(&mesh, &again));
    assert_eq!(mesh.Identity, "triangle.gltf");
    assert_eq!(mesh.Primitives[0].Indices, [0, 1, 2]);
    assert_eq!(mesh.Materials[0].BaseColor, [0.5, 0.25, 0.75, 1.0]);
    let pose = mesh.Pose(None, 0.0).unwrap();
    assert_eq!(
        pose[mesh.FindNode("socket").unwrap()].0[3],
        [2.0, 3.0, 0.0, 1.0]
    );

    std::fs::write(
        root.join("no-indices.gltf"),
        model.replace(r#""indices":1,"#, ""),
    )
    .unwrap();
    let unindexed = library.Load(Path::new("no-indices.gltf")).unwrap();
    assert_eq!(unindexed.Primitives[0].Indices, [0, 1, 2]);
    std::fs::write(
        root.join("outside.gltf"),
        model.replace("mesh%20body.bin", "../outside.bin"),
    )
    .unwrap();
    assert!(
        library
            .Load(Path::new("outside.gltf"))
            .err()
            .unwrap()
            .Detail
            .contains("inside the project")
    );
    std::fs::write(
        root.join("unresolved.gltf"),
        model.replace("mesh%20body.bin", "not-here.bin"),
    )
    .unwrap();
    assert!(matches!(
        library
            .Load(Path::new("unresolved.gltf"))
            .err()
            .unwrap()
            .Stage,
        AssetStage::Buffers
    ));
    std::fs::write(
        root.join("lines.gltf"),
        model.replace(r#""indices":1,"#, r#""indices":1,"mode":1,"#),
    )
    .unwrap();
    assert!(
        library
            .Load(Path::new("lines.gltf"))
            .err()
            .unwrap()
            .Detail
            .contains("only triangles")
    );
    bytes[38] = 99;
    std::fs::write(root.join("mesh body.bin"), bytes).unwrap();
    std::fs::write(root.join("bad-index.gltf"), model).unwrap();
    assert!(
        library
            .Load(Path::new("bad-index.gltf"))
            .err()
            .unwrap()
            .Detail
            .contains("out-of-range index")
    );
}

#[test]
fn pose_tracks_support_linear_shortest_rotation_and_step() {
    let mut track = Track {
        Node: 0,
        Name: None,
        Property: Property::Translation,
        Step: false,
        Times: vec![0.0, 1.0],
        Values: vec![[0.0; 4], [2.0, 4.0, 6.0, 0.0]],
    };
    assert_eq!(track.Sample(0.5), [1.0, 2.0, 3.0, 0.0]);
    track.Step = true;
    assert_eq!(track.Sample(0.5), [0.0; 4]);
    assert_eq!(track.Sample(1.0), track.Values[1]);
    track.Step = false;
    track.Property = Property::Rotation;
    track.Values = vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, -1.0]];
    assert_eq!(track.Sample(0.5), track.Values[0]);
}

#[test]
fn baked_material_vertex_colors_compile_through_sdslv() {
    use crate::Engine::shader::sdslv::CompileCanonicalSourceToShaderArtifact;

    let source = include_str!("../../../../integrations/zed/wyrmcoil_view/src/tactical.sdslv");
    let artifact = CompileCanonicalSourceToShaderArtifact("tactical.sdslv", source).unwrap();
    assert_eq!(artifact.EntryPoints[0].Name, "Tactical_VS");
    assert_eq!(artifact.EntryPoints[1].Name, "Tactical_PS");
    assert!(artifact.Hlsl.contains(">>"));
}

#[test]
fn kaykit_gltf_and_glb_have_indexed_palette_materials_and_evaluated_skin_poses() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/games/last-watch/assets");
    let mut library = AssetLibrary::New(&assets).unwrap();
    let file = Path::new("environment/floor_tile_large.gltf");
    let floor = library.Load(file).unwrap();
    assert!(floor.Skins.is_empty());
    let floor = PaletteMesh::New(floor).unwrap();
    let mut atlas = MeshAtlas::default();
    let range = floor.AppendPose(&mut atlas, None, 0.0).unwrap();
    assert_eq!(range, [0, atlas.Indices.len() as u32, 0]);
    assert!(atlas.Vertices.iter().any(|vertex| vertex[0].abs() > 1.5));
    assert!(
        atlas
            .Vertices
            .iter()
            .any(|vertex| vertex[3] != atlas.Vertices[0][3])
    );

    let file = Path::new("characters/Knight.glb");
    let knight = library.Load(file).unwrap();
    assert_eq!(knight.Skins.len(), 1);
    let knight = PaletteMesh::New(knight).unwrap();
    let clips = library
        .LoadClips(
            Path::new("animations/Rig_Medium_MovementBasic.glb"),
            &["Walking_A"],
        )
        .unwrap();
    let walk = &clips.Clips["Walking_A"];
    let first = knight.AppendPose(&mut atlas, Some(walk), 0.0).unwrap();
    let end = knight
        .AppendPose(&mut atlas, Some(walk), walk.Duration * 0.43)
        .unwrap();
    let count = (end[2] - first[2]) as usize;
    let poses = &atlas.Vertices[first[2] as usize..];
    assert_eq!(poses.len(), count * 2);
    assert!(poses.iter().flatten().all(|x| x.is_finite()));
    assert!(poses.iter().any(|vertex| vertex[1] > 0.80));
    assert!(
        poses[..count]
            .iter()
            .zip(&poses[count..])
            .any(|(a, b)| (a[1] - b[1]).abs() > 0.05)
    );
    assert!(
        poses[..count]
            .iter()
            .zip(&poses[count..])
            .all(|(a, b)| a[3] == b[3])
    );
}
