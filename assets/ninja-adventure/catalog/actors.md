# Ninja Adventure — Actor catalog

Structural index of every actor. Frame layout convention (row = direction/action, col = frame, 16px):

- **64x112** standard: rows 0-3 walk D/U/L/R (×4 frames), row 4 idle, row 5 attack, row 6 jump
- **64x64** simple/monster: rows 0-3 walk D/U/L/R (×4), no action rows
- **SeparateAnim/**: Walk 64x64 (dir×frame); Idle/Attack/Jump 64x16 (4 frames); Dead/Item/Special 16x16
- **Faceset.png** 38x38 dialogue portrait

| Group | Count |
|---|---:|
| Animal | 26 |
| Boss | 20 |
| Character | 92 |
| CharacterAnimated | 2 |
| Monster | 66 |
| **total** | **206** |

**Sheet layouts:** standard-4dir×89, walk-4dir×66, none×28, large-or-irregular×12, strip×8, grid-16×3

## Animal (26)

| Name | Sheet | Layout | Faceset | SepAnim |
|---|---|---|:-:|:-:|
| Cat | 32×16 | strip | ✓ | — |
| CatBlack | 32×16 | strip | ✓ | — |
| CatCyclop | 32×16 | strip | ✓ | — |
| CatOrange | 44×17 | large-or-irregular | ✓ | — |
| CatWhite | 34×15 | large-or-irregular | ✓ | — |
| Chicken | — | — | — | — |
| Cow | — | — | ✓ | — |
| Dog | 32×16 | strip | ✓ | — |
| Dog2 | 32×16 | strip | ✓ | — |
| DogBlack | 36×17 | large-or-irregular | ✓ | — |
| DogOrange | 36×16 | large-or-irregular | ✓ | — |
| DogYellow | 42×17 | large-or-irregular | ✓ | — |
| Donkey | — | — | — | — |
| Fish | — | — | — | — |
| Frog | 32×16 | strip | ✓ | — |
| Hamster | 30×15 | large-or-irregular | ✓ | — |
| Horse | — | — | — | — |
| Hyena | 28×13 | large-or-irregular | ✓ | — |
| Lion | — | — | — | — |
| LionCub | 32×16 | strip | ✓ | — |
| Lioness | — | — | — | — |
| Monkey | — | — | — | — |
| Parrot | — | — | — | — |
| Pig | — | — | — | — |
| Racoon | 32×16 | strip | ✓ | — |
| WildBoar | 34×16 | large-or-irregular | ✓ | — |

## Boss (20)

| Name | Sheet | Layout | Faceset | SepAnim |
|---|---|---|:-:|:-:|
| DemonCyclop | 37×33 | large-or-irregular | ✓ | — |
| DemonCyclop2 | 37×33 | large-or-irregular | ✓ | — |
| DragonBlue | — | — | ✓ | — |
| DragonGreen | — | — | ✓ | — |
| GiantBamboo | — | — | ✓ | — |
| GiantBamboo2 | — | — | ✓ | — |
| GiantBlueSamurai | — | — | ✓ | — |
| GiantFlam | — | — | ✓ | — |
| GiantFrog | — | — | ✓ | — |
| GiantFrog2 | — | — | ✓ | — |
| GiantRacoon | 55×50 | large-or-irregular | ✓ | — |
| GiantRacoonGold | 43×48 | large-or-irregular | ✓ | — |
| GiantRedSamurai | — | — | ✓ | — |
| GiantSlime | — | — | ✓ | — |
| GiantSlime2 | — | — | ✓ | — |
| GiantSpirit | — | — | ✓ | — |
| SquidGreen | — | — | ✓ | — |
| SquidRed | — | — | ✓ | — |
| TenguBlue | — | — | ✓ | — |
| TenguRed | — | — | ✓ | — |

## Character (92)

| Name | Sheet | Layout | Faceset | SepAnim |
|---|---|---|:-:|:-:|
| Boy | 64×112 | standard-4dir | ✓ | ✓ |
| CamouflageGreen | 64×112 | standard-4dir | ✓ | ✓ |
| CamouflageRed | 64×112 | standard-4dir | ✓ | ✓ |
| CaveLion | 64×112 | standard-4dir | ✓ | ✓ |
| CaveLion2 | 64×112 | standard-4dir | ✓ | ✓ |
| Cavegirl | 64×112 | standard-4dir | ✓ | ✓ |
| Cavegirl2 | 64×112 | standard-4dir | ✓ | ✓ |
| Caveman | 64×112 | standard-4dir | ✓ | ✓ |
| Caveman2 | 64×112 | standard-4dir | ✓ | ✓ |
| Child | 64×32 | grid-16 | ✓ | — |
| DemonGreen | 64×112 | standard-4dir | ✓ | ✓ |
| DemonRed | 64×112 | standard-4dir | ✓ | ✓ |
| EggBoy | 64×112 | standard-4dir | ✓ | ✓ |
| EggGirl | 64×112 | standard-4dir | ✓ | ✓ |
| Eskimo | 64×112 | standard-4dir | ✓ | ✓ |
| FighterRed | 64×112 | standard-4dir | ✓ | ✓ |
| FighterWhite | 64×112 | standard-4dir | ✓ | ✓ |
| Flam | 64×112 | standard-4dir | ✓ | ✓ |
| GladiatorBlue | 64×112 | standard-4dir | ✓ | ✓ |
| GoldStatue | 64×112 | standard-4dir | ✓ | ✓ |
| GreenPig | 64×112 | standard-4dir | ✓ | ✓ |
| Hunter | 64×112 | standard-4dir | ✓ | ✓ |
| Inspector | 64×112 | standard-4dir | ✓ | ✓ |
| Knight | 64×112 | standard-4dir | ✓ | ✓ |
| KnightGold | 64×112 | standard-4dir | ✓ | ✓ |
| Lion | 64×112 | standard-4dir | ✓ | ✓ |
| LionBoy | 64×112 | standard-4dir | ✓ | ✓ |
| LionOrange | 64×112 | standard-4dir | ✓ | ✓ |
| LionYellow | 64×112 | standard-4dir | ✓ | ✓ |
| ManGreen | 64×112 | standard-4dir | — | ✓ |
| MaskFrog | 64×112 | standard-4dir | ✓ | ✓ |
| MaskGoldRacoon | 64×112 | standard-4dir | ✓ | ✓ |
| MaskRacoon | 64×112 | standard-4dir | ✓ | ✓ |
| Master | 64×112 | standard-4dir | ✓ | ✓ |
| Monk | 64×112 | standard-4dir | ✓ | ✓ |
| Monk2 | 64×112 | standard-4dir | ✓ | ✓ |
| Monkey | 64×112 | standard-4dir | ✓ | ✓ |
| MonkeyBoxerBlue | 64×112 | standard-4dir | ✓ | ✓ |
| MonkeyBoxerRed | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaBlue | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaBlue2 | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaBomb | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaDark | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaEskimo | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaFire | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaGray | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaGreen | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaLeaf | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaMageBlack | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaMageOrange | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaMasked | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaRed | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaRed2 | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaThunder | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaWater | 64×112 | standard-4dir | ✓ | ✓ |
| NinjaYellow | 64×112 | standard-4dir | ✓ | ✓ |
| Noble | 64×112 | standard-4dir | ✓ | ✓ |
| OldMan | 64×112 | standard-4dir | ✓ | ✓ |
| OldMan2 | 64×112 | standard-4dir | ✓ | ✓ |
| OldMan3 | 64×112 | standard-4dir | ✓ | ✓ |
| OldWoman | 64×32 | grid-16 | ✓ | — |
| Pig | 64×112 | standard-4dir | ✓ | ✓ |
| Princess | 64×112 | standard-4dir | ✓ | ✓ |
| RedGladiator | 64×112 | standard-4dir | ✓ | ✓ |
| RedNinja3 | 64×112 | standard-4dir | ✓ | ✓ |
| RobotCamouflage | 64×112 | standard-4dir | ✓ | ✓ |
| RobotGreen | 64×112 | standard-4dir | ✓ | ✓ |
| RobotGrey | 64×112 | standard-4dir | ✓ | ✓ |
| Samurai | 64×112 | standard-4dir | ✓ | ✓ |
| SamuraiBlue | 64×112 | standard-4dir | ✓ | ✓ |
| SamuraiRed | — | — | ✓ | ✓ |
| Shaman | 64×112 | standard-4dir | ✓ | ✓ |
| ShamanLion | 64×112 | standard-4dir | ✓ | ✓ |
| Skeleton | 64×112 | standard-4dir | ✓ | ✓ |
| SkeletonDemon | 64×112 | standard-4dir | ✓ | ✓ |
| SorcererBlack | 64×112 | standard-4dir | ✓ | ✓ |
| SorcererOrange | 64×112 | standard-4dir | ✓ | ✓ |
| Spirit | 64×112 | standard-4dir | ✓ | ✓ |
| Statue | 64×112 | standard-4dir | ✓ | ✓ |
| Sultan | 64×112 | standard-4dir | ✓ | ✓ |
| Sultan2 | 64×112 | standard-4dir | ✓ | ✓ |
| Tengu | 64×112 | standard-4dir | ✓ | ✓ |
| Tengu2 | 64×112 | standard-4dir | ✓ | ✓ |
| Vampire | 64×112 | standard-4dir | ✓ | ✓ |
| Village6 | 64×112 | standard-4dir | ✓ | ✓ |
| Villager | 64×112 | standard-4dir | ✓ | ✓ |
| Villager2 | 64×112 | standard-4dir | ✓ | ✓ |
| Villager3 | 64×112 | standard-4dir | ✓ | ✓ |
| Villager4 | 64×112 | standard-4dir | ✓ | ✓ |
| Villager5 | 64×112 | standard-4dir | ✓ | ✓ |
| Villager6 | 64×112 | standard-4dir | ✓ | ✓ |
| Woman | 64×112 | standard-4dir | ✓ | ✓ |

## CharacterAnimated (2)

| Name | Sheet | Layout | Faceset | SepAnim |
|---|---|---|:-:|:-:|
| NinjaGreen | 256×544 | grid-16 | — | ✓ |
| Weapon | — | — | — | — |

## Monster (66)

| Name | Sheet | Layout | Faceset | SepAnim |
|---|---|---|:-:|:-:|
| Axolot | 64×64 | walk-4dir | ✓ | — |
| AxolotBlue | 64×64 | walk-4dir | ✓ | — |
| Bamboo | 64×64 | walk-4dir | ✓ | — |
| BambooYellow | 64×64 | walk-4dir | ✓ | — |
| Bear | 64×64 | walk-4dir | ✓ | — |
| Beast | 64×64 | walk-4dir | ✓ | — |
| Beast2 | 64×64 | walk-4dir | ✓ | — |
| BlueBat | 64×64 | walk-4dir | ✓ | — |
| Butterfly | 64×64 | walk-4dir | ✓ | — |
| ButterflyBlue | 64×64 | walk-4dir | ✓ | — |
| Cyclope | 64×64 | walk-4dir | ✓ | — |
| Cyclope2 | 64×64 | walk-4dir | ✓ | — |
| Dragon | 64×64 | walk-4dir | ✓ | — |
| DragonYellow | 64×64 | walk-4dir | ✓ | — |
| Eye | 64×64 | walk-4dir | ✓ | — |
| Eye2 | 64×64 | walk-4dir | ✓ | — |
| Fish | 64×64 | walk-4dir | ✓ | — |
| FishRed | 64×64 | walk-4dir | ✓ | — |
| Flam | 64×64 | walk-4dir | ✓ | — |
| Flam2 | 64×64 | walk-4dir | ✓ | — |
| GoldRacoon | 64×64 | walk-4dir | ✓ | — |
| GreenOctopus | 64×64 | walk-4dir | ✓ | — |
| Grey Trex | 64×64 | walk-4dir | ✓ | — |
| HeartGreen | 64×64 | walk-4dir | ✓ | — |
| HeartRed | 64×64 | walk-4dir | ✓ | — |
| KappaGreen | 64×64 | walk-4dir | ✓ | — |
| KappaRed | 64×64 | walk-4dir | ✓ | — |
| LanternGreen | 64×64 | walk-4dir | ✓ | — |
| LanternRed | 64×64 | walk-4dir | ✓ | — |
| Larva | 64×64 | walk-4dir | ✓ | — |
| Larva2 | 64×64 | walk-4dir | ✓ | — |
| Lizard | 64×64 | walk-4dir | ✓ | — |
| Lizard2 | 64×64 | walk-4dir | ✓ | — |
| Mole | 64×64 | walk-4dir | ✓ | — |
| Mole2 | 64×64 | walk-4dir | ✓ | — |
| Mollusc | 64×64 | walk-4dir | ✓ | — |
| Mollusc2 | 64×64 | walk-4dir | ✓ | — |
| Mouse | 64×64 | walk-4dir | ✓ | — |
| MouseBlack | 64×64 | walk-4dir | ✓ | — |
| Mushroom | 64×64 | walk-4dir | ✓ | — |
| Mushroom2 | 64×64 | walk-4dir | ✓ | — |
| Octopus | 64×64 | walk-4dir | ✓ | — |
| Octopus2 | 64×64 | walk-4dir | ✓ | — |
| Owl | 64×64 | walk-4dir | ✓ | — |
| Owl2 | 64×64 | walk-4dir | ✓ | — |
| Panda | 64×64 | walk-4dir | ✓ | — |
| Racoon | 64×64 | walk-4dir | ✓ | — |
| RedOctopus | 64×64 | walk-4dir | ✓ | — |
| Reptile | 64×64 | walk-4dir | ✓ | — |
| Reptile2 | 64×64 | walk-4dir | ✓ | — |
| Skull | 64×64 | walk-4dir | ✓ | — |
| SkullBlue | 64×64 | walk-4dir | ✓ | — |
| Slime | 64×64 | walk-4dir | ✓ | — |
| Slime2 | 64×64 | walk-4dir | ✓ | — |
| Slime3 | 64×64 | walk-4dir | ✓ | — |
| Slime4 | 64×64 | walk-4dir | ✓ | — |
| Snake | 64×64 | walk-4dir | ✓ | — |
| Snake2 | 64×64 | walk-4dir | ✓ | — |
| Snake3 | 64×64 | walk-4dir | ✓ | — |
| Snake4 | 64×64 | walk-4dir | ✓ | — |
| SpiderRed | 64×64 | walk-4dir | ✓ | — |
| SpiderYellow | 64×64 | walk-4dir | ✓ | — |
| Spirit | 64×64 | walk-4dir | ✓ | — |
| Spirit2 | 64×64 | walk-4dir | ✓ | — |
| TRex | 64×64 | walk-4dir | ✓ | — |
| YellowsBat | 64×64 | walk-4dir | ✓ | — |
