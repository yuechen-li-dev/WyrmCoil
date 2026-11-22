# The Last Watch asset credits

Kay Lousberg created the KayKit assets below. The publisher distributes
each listed free release under [CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/).
Downloaded from the author's official itch.io pages on 2026-11-22.
Only the files used in the encounter were copied. All source files are
byte-for-byte copies from the free archives.

| Pack | Original release | Files used in this project |
| --- | --- | --- |
| KayKit Dungeon Pack | [Free 1.1](https://kaylousberg.itch.io/kaykit-dungeon-pack) | `environment/{floor_tile_large,wall,pillar,chest_gold}.{gltf,bin}` and `environment/dungeon_texture.png` |
| KayKit Adventurers | [Free 2.0](https://kaylousberg.itch.io/kaykit-adventurers) | `characters/{Knight,Rogue,Mage,Ranger,Barbarian}.glb`, `equipment/{sword_1handed,shield_badge,dagger,staff}.{gltf,bin}` and `equipment/{knight,rogue,mage}_texture.png` |
| KayKit Character Animations | [Free 1.1](https://kaylousberg.itch.io/kaykit-character-animations) | `animations/Rig_Medium_{General,MovementBasic,CombatMelee,CombatRanged}.glb` |

At runtime WyrmCoil reads the original meshes and textures, samples the
palette textures to vertex colors, and evaluates the original medium-rig
clips into cached render poses. The game applies alternative enemy tints
at draw time. Please honor the artist's request about standalone resale
and claiming authorship.

## Archive verification

Each archive passed a full ZIP CRC check. Original archives remain under
the ignored `target/m8-downloads` directory in the qualification checkout.

| Archive | Bytes | SHA-256 |
| --- | ---: | --- |
| Dungeon 1.1 | 32,868,755 | `6acb859d1aefae074f937d1e6f13656a7312b2bdb6d9f1232ebdade8d93d6a1c` |
| Adventurers 2.0 | 13,024,345 | `abe48f4763fba0896bab486ee9e6d08ca6b5b3884b9601f235c8847ae94dc479` |
| Animations 1.1 | 14,858,957 | `65882f31f905ad2e953819648a59287cdeab8f623908d5ef701971d3758be20f` |
