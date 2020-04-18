# reMarkable-tools
Tools for the reMarkable paper tablet

## koreader
https://github.com/koreader/koreader/wiki/Installation-on-Remarkable

## Tools
* [`cbr2cbz.sh *.cbr`](./cbr2cbz.sh) unrars then zips CBRs into CBZs so `koreader` can open them.
* `rsync` on the tablet:
    1. wget https://raw.githubusercontent.com/Evidlo/remarkable_entware/master/entware_install.sh && chmod +x entware_install.sh
    1. opkg install rsync
    1. And because `ssh remarkable 'echo $PATH' #=> /usr/bin:/bin`
    1. ln -s `which rsync` /usr/bin/

## Attention

### Updates wipe `~`

### root partition is small
```
remarkable: ~/ df -h
Filesystem                Size      Used Available Use% Mounted on
/dev/root               223.0M    175.0M     32.3M  84% /
```
so routinely run
```
journalctl --vacuum-size=2M
```
