# reMarkable-tools
Tools for the reMarkable paper tablet

## koreader
* https://github.com/koreader/koreader/releases/latest
* instructions: https://github.com/koreader/koreader/wiki/Installation-on-Remarkable
* creates metadata files:
```
find ~ -name '*.sdr'
```

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

## marauder

* [![Marauder's map](https://thumbs.gfycat.com/AcrobaticLastingBeardedcollie-mobile.jpg)](https://zippy.gfycat.com/AcrobaticLastingBeardedcollie.webm)
* https://github.com/ax3l/lines-are-rusty
	* https://github.com/reHackable/maxio/blob/master/tools/rM2svg
* datasets & models for online writting & drawings
	* task: HTR = Handwriting Text Recognition
	* https://quickdraw.withgoogle.com
	* https://archive.ics.uci.edu/ml/datasets/UJI+Pen+Characters
		* https://archive.ics.uci.edu/ml/datasets/Pen-Based+Recognition+of+Handwritten+Digits
	* https://arxiv.org/abs/1904.08095
	* https://arxiv.org/abs/1907.12935
	* https://www.gavo.t.u-tokyo.ac.jp/~qiao/database.html
	* http://www.wikicfp.com/cfp/program?id=1366&f=International%20Conference%20on%20Frontiers%20in%20Handwriting%20Recognition
	* https://en.wikipedia.org/wiki/List_of_datasets_for_machine-learning_research#Handwriting_and_character_recognition
	* https://mathpix.com/
* https://github.com/dickrnn/dickrnn.github.io
* https://github.com/tonybeltramelli/pix2code
* https://crates.io/crates/eliza
* https://parl.ai/projects/recipes
* https://billwadge.wordpress.com/2020/04/20/the-intensional-spreadsheet
* https://github.com/lisbravo/MNIST-drawing-test
