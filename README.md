# reMarkable-tools
Tools for the [reMarkable paper tablet](https://remarkable.com/) that I or others develop.

## Donate

Feel free to donate to me through [paypal.me/pierrefenoll1](paypal.me/pierrefenoll1)  
Make sure to describe what I should be working on :)

## Whiteboard HyperCard ~ live collaboration/drawing/chat/whiteboarding

Easiest installation through [toltec's `opkg`](https://github.com/toltec-dev/toltec)
```
opkg update && opkg install whiteboard_hypercard
```

### Self-hosting whiteboard-server / hosting private rooms

On a machine with IP `1.2.3.4` reachable over the Internet, run:
```
git clone https://github.com/fenollp/reMarkable-tools.git && cd reMarkable-tools && docker-compose up --build --abort-on-container-exit
```
Now on your tablet, run the `whiteboard` Rust application with `--host`, as in:
```
.../whiteboard --host=http://1.2.3.4:10000
```
Finally, `docker-compose` should show you something akin to:
```
nats_1        | [1] 2020/11/03 14:26:24.435123 [DBG] 172.20.0.3:60308 - cid:1 - Client Ping Timer
nats_1        | [1] 2020/11/03 14:26:24.435145 [DBG] 172.20.0.3:60308 - cid:1 - Delaying PING due to remote ping 2s ago
nats_1        | [1] 2020/11/03 14:28:22.270230 [TRC] 172.20.0.3:60308 - cid:1 - <<- [PING]
nats_1        | [1] 2020/11/03 14:28:22.270306 [TRC] 172.20.0.3:60308 - cid:1 - ->> [PONG]
nats_1        | [1] 2020/11/03 14:28:24.435532 [DBG] 172.20.0.3:60308 - cid:1 - Client Ping Timer
nats_1        | [1] 2020/11/03 14:28:24.435701 [DBG] 172.20.0.3:60308 - cid:1 - Delaying PING due to remote ping 2s ago
wb            | 2020-11-03T14:28:41.402Z	INFO	hypercard_whiteboard/rpc_recv_events.go:32	handling RecvEvent	{"": "c91dd90e-77b8-477c-94f7-a25ff0e5b584"}
wb            | 2020-11-03T14:28:41.402Z	DEBUG	hypercard_whiteboard/rpc_recv_events.go:46	listening for events	{"": "c91dd90e-77b8-477c-94f7-a25ff0e5b584", "bk": "hc.wb.1.evt.living-room.*.*"}
wb            | 2020-11-03T14:28:41.402Z	DEBUG	hypercard_whiteboard/nats.go:44	encoding	{"": "c91dd90e-77b8-477c-94f7-a25ff0e5b584", "event": {"created_at":1604413721402953665,"by_user_id":"c91dd90e-77b8-477c-94f7-a25ff0e5b584","in_room_id":"living-room","Event":{"UserJoinedTheRoom":true}}}
wb            | 2020-11-03T14:28:41.403Z	DEBUG	hypercard_whiteboard/nats.go:50	encoded	{"": "c91dd90e-77b8-477c-94f7-a25ff0e5b584", "bytes": 63, "in": "160.551µs"}
wb            | 2020-11-03T14:28:41.403Z	DEBUG	hypercard_whiteboard/nats.go:56	publishing	{"": "c91dd90e-77b8-477c-94f7-a25ff0e5b584", "rk": "hc.wb.1.evt.living-room.c91dd90e-77b8-477c-94f7-a25ff0e5b584.userjoinedroom"}
wb            | 2020-11-03T14:28:41.403Z	DEBUG	hypercard_whiteboard/nats.go:62	published	{"": "c91dd90e-77b8-477c-94f7-a25ff0e5b584", "rk": "hc.wb.1.evt.living-room.c91dd90e-77b8-477c-94f7-a25ff0e5b584.userjoinedroom", "in": "6.926µs"}
nats_1        | [1] 2020/11/03 14:28:41.403146 [TRC] 172.20.0.3:60308 - cid:1 - <<- [SUB hc.wb.1.evt.living-room.*.*  1]
nats_1        | [1] 2020/11/03 14:28:41.403446 [TRC] 172.20.0.3:60308 - cid:1 - <<- [PUB hc.wb.1.evt.living-room.c91dd90e-77b8-477c-94f7-a25ff0e5b584.userjoinedroom 63]
nats_1        | [1] 2020/11/03 14:28:41.403472 [TRC] 172.20.0.3:60308 - cid:1 - <<- MSG_PAYLOAD: ["\b\xc1\x9f\xf1\x87\xf7\xb8\x81\xa2\x16\x12$c91dd90e-77b8-477c-94f7-a25ff0e5b584\x1a\vliving-room0\x01"]
nats_1        | [1] 2020/11/03 14:28:41.403491 [TRC] 172.20.0.3:60308 - cid:1 - ->> [MSG hc.wb.1.evt.living-room.c91dd90e-77b8-477c-94f7-a25ff0e5b584.userjoinedroom 1 63]
wb            | 2020-11-03T14:28:41.404Z	DEBUG	hypercard_whiteboard/rpc_recv_events.go:100	sent count event	{"": "c91dd90e-77b8-477c-94f7-a25ff0e5b584", "in": "73.035µs"}
```

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

## HyperCards

Visible rectangular elements that can be drawn/moved/zoomed/rotated/connected/duplicated.
* tools / modifiers are square cards snapping to the edges of the screen
	* *henceforth mentioned as `[ToolX]` for tool X*
	* using them / modifying them by a press held while using the pencil
	* a `[?]` tool always hangs in a corner, pressing = shows description text (like crosswords)
	* modification (reorganization / addition / removal) through drawing
	* tools icons can be drawn too / loaded from a font / loaded from a builtin set of images
* whiteboard card
	* rectangle that can be moved by dragging on the edges
	* zoom/rotation by dragging in the area (not the edges)
		* MIGHT: zooming hard moves to another user's view?
	* some bi-directional communication with a networked service
		* user joins a room and shares their live drawings
		* It is possible to combine a [Selection]-ed group of strokes and [Digitize] to ask service for translation
* `[Digitize]`
	* connects to a distant machine or achieves its AI inference on-tablet
	* takes a few strokes in and outputs text+area / shape+area
* `[Selection]`
	* draw approximately on one or more strokes
	* creates a group that can be used with other modifiers
	* press another tool before unpressing this one to pass the group to the other tool

## marauder

* [![Marauder's map](https://thumbs.gfycat.com/AcrobaticLastingBeardedcollie-size_restricted.gif)](https://zippy.gfycat.com/AcrobaticLastingBeardedcollie.webm)
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
* https://supernote.com/#/product?type=SN-A5
* https://www.myscript.com/
	* https://github.com/MyScript/interactive-ink-examples-ios
	* https://github.com/CocoaPods/Specs/blob/master/Specs/f/c/3/MyScriptInteractiveInk-Runtime
* https://untools.co/
* https://en.wikipedia.org/wiki/TRIZ
* https://eugeneyan.com/2020/04/05/note-taking-zettelkasten/
* https://github.com/alexandre01/deepsvg
* https://ai.googleblog.com/2020/11/using-gans-to-create-fantastical.html
