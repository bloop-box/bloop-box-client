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
