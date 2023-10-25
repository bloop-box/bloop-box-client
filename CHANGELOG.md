# [4.0.0](https://github.com/bloop-box/bloop-box-client/compare/v3.2.0...v4.0.0) (2023-10-25)


### Bug Fixes

* **wifi:** use correct command to set credentials ([875f28f](https://github.com/bloop-box/bloop-box-client/commit/875f28ff5a373114a48e860416c19c09c5fa0bff))


### Features

* **controller:** change busy color to magenta to adjust for color blindness ([dc58e59](https://github.com/bloop-box/bloop-box-client/commit/dc58e59f36058b53bf9ebdaa0bc4277b48eaf9a4))
* implement bloop version 2 protocol ([5147b2f](https://github.com/bloop-box/bloop-box-client/commit/5147b2fb1f2d7efcdc9a7c8bc2bd489eeac4f73b))
* replace wpactl with nmcli ([b014ac7](https://github.com/bloop-box/bloop-box-client/commit/b014ac7639d4e23cd6d7b661fe93dc18b3082f94))


### BREAKING CHANGES

* This will require Raspbian Bookworm or newer.
* This will require a server implementing the bloop version 2
protocol.

# [3.2.0](https://github.com/bloop-box/bloop-box-client/compare/v3.1.0...v3.2.0) (2023-02-18)


### Bug Fixes

* **AudioPlayer:** remove testing message ([2607e47](https://github.com/bloop-box/bloop-box-client/commit/2607e47ee6ac3c2ef79fff397100ea896a95fbec))
* **debian:** suppress all info messages from third party libs ([ab63a9a](https://github.com/bloop-box/bloop-box-client/commit/ab63a9af057a51850425e71047ea9a15ac624f89))


### Features

* shut down all background tasks on shutdown ([e16c7b4](https://github.com/bloop-box/bloop-box-client/commit/e16c7b4c8d66c7fa64533fa46f3bb8cabce1946f))

# [3.1.0](https://github.com/bloop-box/bloop-box-client/compare/v3.0.0...v3.1.0) (2023-02-18)


### Bug Fixes

* **nfc:** initialize mfrc522 after reset ([20a8e90](https://github.com/bloop-box/bloop-box-client/commit/20a8e90325d282c5ec461dfd86cd014a7dca075e))


### Features

* **AudioPlayer:** allow weighted random selection ([de4190a](https://github.com/bloop-box/bloop-box-client/commit/de4190a841159c84ec090a330c5af8337e5e79b6))

# [3.0.0](https://github.com/bloop-box/bloop-box-client/compare/v2.1.0...v3.0.0) (2023-02-04)


### Bug Fixes

* **audio_player:** re-open audio player each time a sound is played ([af7fd5f](https://github.com/bloop-box/bloop-box-client/commit/af7fd5f7be82afdd1ad3938b94d79faa224af94a))
* **clippy:** use format variables in-line ([75ea445](https://github.com/bloop-box/bloop-box-client/commit/75ea4453c492c4a47f5412cc02e312d14cd4036d))
* **networker:** use async DNS lookup ([27cc667](https://github.com/bloop-box/bloop-box-client/commit/27cc667bcf19049e2e6570516478d5a5d60bc75a))
* **nfc:** do not crash when responder was dropped ([9f39246](https://github.com/bloop-box/bloop-box-client/commit/9f39246fd7b0ba9c84ec7c83233461dd42808faa))


### Features

* allow disabling certification verification for development ([e57e5b0](https://github.com/bloop-box/bloop-box-client/commit/e57e5b07088dc75fe81c06db89d56ad95492550e))
* **led:** replace GPIO based LED with AW2013 ([3884921](https://github.com/bloop-box/bloop-box-client/commit/38849214e3e9566f7144bc4eff3833afc1c88ad9))


### BREAKING CHANGES

* **led:** You must use newer Bloop Box hardware with support for the
AW2013 chipset.

# [2.1.0](https://github.com/bloop-box/bloop-box-client/compare/v2.0.4...v2.1.0) (2022-12-26)


### Bug Fixes

* **controller:** inverse check for networker status being connected ([3cf3e07](https://github.com/bloop-box/bloop-box-client/commit/3cf3e0758057ba5fbdc94e766ee34745184a47e0))
* **nfc:** ignore error on poll send result ([3614240](https://github.com/bloop-box/bloop-box-client/commit/3614240c6763d524c06d45ab2457ade2e3bfb19e))


### Features

* let service unit wait for spidev0.0 to be available ([002ba51](https://github.com/bloop-box/bloop-box-client/commit/002ba514d0211fdb894027bd3cd7c58f0b7e92f4))

## [2.0.4](https://github.com/bloop-box/bloop-box-client/compare/v2.0.3...v2.0.4) (2022-12-26)


### Bug Fixes

* **controller:** do not accept non config uids when not connected ([1b035f7](https://github.com/bloop-box/bloop-box-client/commit/1b035f7d7992bbfab1912144ad05b38d12cb4209))
* **nfc:** check twice for card release ([e4a2a80](https://github.com/bloop-box/bloop-box-client/commit/e4a2a80e66f1ab86745cf86ac7753086dc0c0fb1))

## [2.0.3](https://github.com/bloop-box/bloop-box-client/compare/v2.0.2...v2.0.3) (2022-12-26)


### Bug Fixes

* **wifi:** add user to netdev group ([bdc12e1](https://github.com/bloop-box/bloop-box-client/commit/bdc12e1a50e21b5ac062f7f61ab0d3e47362d3a8))

## [2.0.2](https://github.com/bloop-box/bloop-box-client/compare/v2.0.1...v2.0.2) (2022-12-25)


### Bug Fixes

* **release:** set owner and group to 0 when re-packaging data.tar.xz ([99c8590](https://github.com/bloop-box/bloop-box-client/commit/99c8590f26caf43f34b8c9c2345cad3f6c4cfd5f))

## [2.0.1](https://github.com/bloop-box/bloop-box-client/compare/v2.0.0...v2.0.1) (2022-12-22)


### Bug Fixes

* add trailing slash to debian sudoers.d deployment ([3c816b2](https://github.com/bloop-box/bloop-box-client/commit/3c816b2a9fd628d386308544a926192540594f60))

# [2.0.0](https://github.com/bloop-box/bloop-box-client/compare/v1.2.1...v2.0.0) (2022-12-19)


### Features

* add shutdown config command ([711147a](https://github.com/bloop-box/bloop-box-client/commit/711147a908e32bec764f20df35e23601dea715f9))
* **nfc:** replace libnfc with mfrc522 driver to drive rc522 chip ([c27dc7e](https://github.com/bloop-box/bloop-box-client/commit/c27dc7e2ad2bfdff587a799d21e0ab95be3e19de))


### BREAKING CHANGES

* **nfc:** As the pn532 chip had issues with reading NTAGs reliably, the
bloop box system was changed over to the rc522 chip. The NFC functionality is
now completely handled by the native mfrc522 library instead of libnfc.

## [1.2.1](https://github.com/bloop-box/bloop-box-client/compare/v1.2.0...v1.2.1) (2022-12-08)


### Bug Fixes

* fix various small issues ([6bcc579](https://github.com/bloop-box/bloop-box-client/commit/6bcc579fb2639d5e21d7442e5e6bbad4e1549c95))

# [1.2.0](https://github.com/bloop-box/bloop-box-client/compare/v1.1.0...v1.2.0) (2022-12-01)


### Features

* replace old mifare classic data reader with ntag reader ([1b5379c](https://github.com/bloop-box/bloop-box-client/commit/1b5379c4dcc999554df970b4447df64b36cb53d5))

# [1.1.0](https://github.com/bloop-box/bloop-box-client/compare/v1.0.2...v1.1.0) (2022-11-30)


### Features

* add etc config to control GPIO pin layout ([6e0a294](https://github.com/bloop-box/bloop-box-client/commit/6e0a294200cc1d6366a1b2a81a33fb8d0ceb583e))

## [1.0.2](https://github.com/bloop-box/bloop-box-client/compare/v1.0.1...v1.0.2) (2022-11-29)


### Bug Fixes

* store state information in /var/lib/bloop-box ([746d28a](https://github.com/bloop-box/bloop-box-client/commit/746d28a3e5857435859830e2b42fc919c5f5f15d))

## [1.0.1](https://github.com/bloop-box/bloop-box-client/compare/v1.0.0...v1.0.1) (2022-11-28)


### Bug Fixes

* **debian:** add bloop-box user to required groups ([4ec2106](https://github.com/bloop-box/bloop-box-client/commit/4ec21062ac5ef0bb6ce640e5118b337e0883e20c))

# 1.0.0 (2022-11-28)


### Bug Fixes

* catch nfc read timeout and fix config storage ([b2a7291](https://github.com/bloop-box/bloop-box-client/commit/b2a7291f6aa810a21c4a114889f88ca2b832d1d8))
* **config_manager:** update local config variable on change ([eb40030](https://github.com/bloop-box/bloop-box-client/commit/eb4003029927e8b495f998e9158170738eb69171))
* **controller:** cancel nfc poll when selecting other future ([e23aeb9](https://github.com/bloop-box/bloop-box-client/commit/e23aeb97bb8e33a92e8e73cd9ba56e30beefb0a1))
* correct minor mistakes in networker ([081ce6c](https://github.com/bloop-box/bloop-box-client/commit/081ce6cc29325ef249dce2a561bc374f43c2a21f))
* **wpa_supplicant:** reconfigure network after changing config ([6c58627](https://github.com/bloop-box/bloop-box-client/commit/6c5862786be1fdc60230a2c314c92fb2cf1e0dff))


### Features

* add networker ([4694c97](https://github.com/bloop-box/bloop-box-client/commit/4694c9734222a3d72520a7b90c8485c69b0b9891))
* initial commit ([b550d29](https://github.com/bloop-box/bloop-box-client/commit/b550d298505cbbbc4f1bb173aeaaea69d8bc9f0b))
* rename to bloop-box and move data files to separate package ([8b9d5e0](https://github.com/bloop-box/bloop-box-client/commit/8b9d5e0c7e8af27da9e81c0cdd2b52b95fc53b01))
