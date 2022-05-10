# reMarkable-tools
Tools for the [reMarkable paper tablet](https://remarkable.com/) that I or others develop.

## Donate

Feel free to donate to me through [paypal.me/pierrefenoll1](https://www.paypal.com/paypalme/pierrefenoll1)  
Make sure to describe what I should be working on :)

## Whiteboard HyperCard ~ live collaboration/drawing/chat/whiteboarding

Easiest installation through [toltec's `opkg`](https://github.com/toltec-dev/toltec)
```
opkg update && opkg install whiteboard-hypercard
```

### Self-hosting whiteboard-server / hosting private rooms

On a machine with IP `1.2.3.4` reachable over the Internet, run:
```
git clone https://github.com/fenollp/reMarkable-tools.git && cd reMarkable-tools && make debug
```
Now on your tablet, run the `whiteboard` Rust application with `--host`, as in:
```
export WHITEBOARD_WEBHOST=http://1.2.3.4:10001/screenshare
.../whiteboard --host=http://1.2.3.4:10000
```
Finally, `docker compose` should show you something akin to:
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

Visible rectangular elements that can be drawn on/dragged/zoomed/rotated/connected/duplicated.
* tools / modifiers are square cards snapping to the edges of the screen
	* *henceforth mentioned as `[ToolX]` for tool X*
	* using them / modifying them by a press held while using the pencil
            * similar to Minecraft's mix & match: action through combination
	* a `[?]` tool always hangs in a corner, pressing = shows description text (like crosswords)
	* modification (reorganization / addition / removal) through drawing and pressing
	* tools icons can be drawn too / loaded from a font / loaded from a builtin set of images
	* pen & fingers are different devices for different purposes
	    * => drag/move and pinch/zoom (think Apple trackpad gestures) not a pen thing
* whiteboard card
	* rectangle that can be moved by dragging on the edges
	* zoom/rotation by dragging in the area (not the edges)
		* MIGHT: zooming hard moves to another user's view? --> canvas is a window/camera view that can move in 2+1D
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
* `[Image]`
	* add an image to a layer
        * should be able to dim that layer
	* select the tool's image with `[Setter] > [Image]` --> opens image picker
	* image should be draggable + zoomable (= two-finger gesture on iOS Reddit app's image viewer)
        * should be able to draw on top of said image

## marauder

* [![Marauder's map](https://thumbs.gfycat.com/AcrobaticLastingBeardedcollie-size_restricted.gif)](https://zippy.gfycat.com/AcrobaticLastingBeardedcollie.webm)
* https://github.com/ax3l/lines-are-rusty
	* https://github.com/reHackable/maxio/blob/master/tools/rM2svg
* datasets & models for online writting & drawings
	* task: HTR = Handwriting Text Recognition
	* https://quickdraw.withgoogle.com
	* https://archive.ics.uci.edu/ml/datasets/UJI+Pen+Characters
		* https://archive.ics.uci.edu/ml/datasets/Pen-Based+Recognition+of+Handwritten+Digits
	* [19, TextCaps : Handwritten Character Recognition with Very Small Datasets](https://arxiv.org/abs/1904.08095)
	* [19, RNN-based Online Handwritten Character Recognition Using Accelerometer and Gyroscope Data](https://arxiv.org/abs/1907.12935)
	* https://www.gavo.t.u-tokyo.ac.jp/~qiao/database.html
	* [ICFHR: International Conference on Frontiers in Handwriting Recognition](http://www.wikicfp.com/cfp/program?id=1366&f=International%20Conference%20on%20Frontiers%20in%20Handwriting%20Recognition)
	* https://en.wikipedia.org/wiki/List_of_datasets_for_machine-learning_research#Handwriting_and_character_recognition
	* https://mathpix.com/
* https://github.com/dickrnn/dickrnn.github.io
* https://github.com/tonybeltramelli/pix2code
* https://crates.io/crates/eliza
* https://parl.ai/projects/recipes
* https://billwadge.wordpress.com/2020/04/20/the-intensional-spreadsheet
* https://github.com/lisbravo/MNIST-drawing-test
* https://www.myscript.com/
	* https://github.com/MyScript/interactive-ink-examples-ios
	* https://github.com/CocoaPods/Specs/blob/master/Specs/f/c/3/MyScriptInteractiveInk-Runtime
* [untools: Tools for better thinking](https://untools.co/)
* [TRIZ: theory of inventive problem solving](https://en.wikipedia.org/wiki/TRIZ)
* [Stop Taking Regular Notes; Use a Zettelkasten Instead](https://eugeneyan.com/2020/04/05/note-taking-zettelkasten/)
* [DeepSVG: A Hierarchical Generative Network for Vector Graphics Animation](https://github.com/alexandre01/deepsvg)
* [Using GANs to Create Fantastical Creatures](https://ai.googleblog.com/2020/11/using-gans-to-create-fantastical.html)
* https://github.com/MarkMoHR/Awesome-Sketch-Based-Applications
* https://github.com/topics/drawing
* [Sketch-Based-Deep-Learning](https://github.com/qyzdao/Sketch-Based-Deep-Learning)
    * [![A-Benchmark-for-Rough-Sketch-Cleanup](https://pbs.twimg.com/media/EjvgmDyWAAE05JD?format=jpg&name=orig)](https://github.com/Nauhcnay/A-Benchmark-for-Rough-Sketch-Cleanup)
    * [BézierSketch: A generative model for scalable vector sketches](https://arxiv.org/abs/2007.02190)
    * [Sketch-R2CNN: An Attentive Network for Vector Sketch Recognition](https://arxiv.org/abs/1811.08170)
* [Detecting hand drawn flowcharts](https://github.com/Ruturaj123/Flowchart-Detection)
* [Deep Sketch-guided Cartoon Video Synthesis](https://arxiv.org/abs/2008.04149)
* [CoSE: While previous approaches rely on sequence-based models for drawings of basic objects or handwritten text, we propose a model that treats drawings as a collection of strokes](https://eth-ait.github.io/cose/)
* https://github.com/MarkMoHR/Awesome-Sketch-Synthesis
* https://github.com/topics/vector-sketch
    * [Animated Construction of Line Drawings](http://sweb.cityu.edu.hk/hongbofu/projects/animatedConstructionOfLineDrawings_SiggA11/)
        * [Contour Drawing Dataset / Photo-Sketching: Inferring Contour Drawings from Images](https://www.cs.cmu.edu/~mengtial/proj/sketch/)
    * [Scones: Towards Conversational Authoring of Sketches](http://people.eecs.berkeley.edu/~eschoop/docs/scones.pdf)
    * [Free-hand sketch synthesis with deformable stroke models](https://panly099.github.io/skSyn.html)
    * [Convert images to vectorized line drawings for plotters.](https://github.com/LingDong-/linedraw)
* [Draw.io and Terraform = Brainboard, Graphical Way to Do Terraform](https://news.ycombinator.com/item?id=25536133)

* [Handwrite generates a custom font based on your handwriting sample](https://github.com/cod-ed/handwrite)
* [Potrace: Transforming bitmaps into vector graphics](http://potrace.sourceforge.net/)
* [21, Im2Vec: Synthesizing Vector Graphics without Vector Supervision](https://arxiv.org/abs/2102.02798v1)

* ![Tally marks (from around the world) as battery bar](https://i.redd.it/qgr5kte3gak51.jpg)

* [Create single line SVG illustrations from your pictures](https://github.com/javierbyte/pintr)
* [Show HN: Tool that turns your images into plotter-like line drawings](https://news.ycombinator.com/item?id=27224094)
* [Pipes and Paper: Ancient Abstractions (or: hacking my ReMarkable tablet into a live presentation tool)](https://blog.afandian.com/2020/10/pipes-and-paper-remarkable/)
* [EasyOCR is a python module for extracting text from image](https://www.jaided.ai/easyocr/)
* [Plan2Scene: Converting Floorplans to 3D Scenes](https://3dlg-hcvc.github.io/plan2scene/)


* [![Elementary Cellular Automata](https://cdn.shopify.com/s/files/1/0300/8102/4131/products/42-30-40891_2048x.png?v=1577735531)](https://store.michaelfogleman.com/products/elementary-cellular-automata)
* [Protein Ribbon Diagrams](https://github.com/fogleman/ribbon)
* [code for generating topographic contour maps](https://github.com/fogleman/terrarium)
* [construct tilings of regular polygons and their dual](https://github.com/fogleman/Tiling)
	* https://en.wikipedia.org/wiki/List_of_Euclidean_uniform_tilings
* [![3D line art engine](https://camo.githubusercontent.com/9cf56a33772e582e53db12702a5809ac912382babc4bba25d494500ec53bad10/687474703a2f2f692e696d6775722e636f6d2f485932466732742e706e67)](https://github.com/fogleman/ln)
* [Turtle graphics is a key feature of the Logo programming language](https://en.wikipedia.org/wiki/Turtle_graphics)
* ![bla](https://pbs.twimg.com/media/ErkHD2xXcAUPMtq?format=png&name=orig)
* [A088218: Total number of leaves in all rooted ordered trees with n edges](https://oeis.org/A088218)

* https://mlajtos.mu/posts/new-kind-of-paper

* [n8n: node based Workflow Automation Tool](https://github.com/n8n-io/n8n)

* [CHAIKIN’S ALGORITHM – DRAWING CURVES](https://www.bit-101.com/blog/2021/08/chaikins-algorithm-drawing-curves/)

* [armrest: handwriting recognition + Elm-inspired UI library](https://github.com/bkirwi/armrest)

* https://www.reddit.com/r/Handwriting_Analysis/

* [demo: offline OCR with Tesseract](https://old.reddit.com/r/RemarkableTablet/comments/jj5yt2/offline_ocr_on_device_no_cloud_no_ssh_on_rm2/)

* [Chalktalk is a digital presentation and communication language in development at New York University's Future Reality Lab](https://github.com/kenperlin/chalktalk)

* [resvg 0.7 - an SVG rendering library](https://www.reddit.com/r/rust/comments/c2m8t7/resvg_07_an_svg_rendering_library/)
	* https://github.com/RazrFalcon/svgtypes/tree/master/fuzz
	* https://github.com/RazrFalcon/resvg

* [librecalibrate button example](https://pastebin.com/KqswUMZF)

* [Pyflow: visual and modular block programming](https://github.com/Bycelium/PyFlow)
* https://store.steampowered.com/app/619150/while_True_learn/
* [rete: visual programming and creating node editor](https://github.com/retejs/rete)
* https://blockprotocol.org/hub

* [A procedural, node-based modelling tool, made in rust](https://github.com/setzer22/blackjack)
	* https://github.com/setzer22/egui_node_graph

* remarkable chemist app: draw molecules on a hex grid so it renders 3d views

* [favorite browser-based creative arts tools/playthings that use AI or Machine Learning](https://twitter.com/golan/status/1496311115571212294)

* **devices**
	* [Ratta Supernote](https://supernote.com/#/product?type=SN-A5)
	* [HUAWEI MatePad Paper](https://consumer.huawei.com/en/tablets/matepad-paper/)
	* [PineNote](https://www.pine64.org/pinenote/)
	* [Reinkstone R1](https://reinkstone.com/collections/reinkstone-r1)

* [forget-me-node: draw lines, get flowers](https://www.reddit.com/r/blender/comments/t5qpas/forgetmenode/)

* [savage: A primitive computer algebra system](https://github.com/p-e-w/savage)

* [FEA in order to simulate physical phenomena in the VIRTUAL world](https://www.reddit.com/r/The3DPrintingBootcamp/comments/tfa6t7/finite_element_analysis_fea_in_3d_printing_more/)

* [19, Shrubbery-shell inspired 3D model stylization](https://www.sciencedirect.com/science/article/abs/pii/S0097849319300561)

* [StyleNeRF: A Style-based 3D-Aware Generator for High-resolution Image Synthesis](https://www.reddit.com/r/learnmachinelearning/comments/ti81x8/stylenerf_a_stylebased_3daware_generator_for/)

* [λ-2D: An Exploration of Drawing as Programming Language, Featuring Ideas from Lambda Calculus](https://www.media.mit.edu/projects/2d-an-exploration-of-drawing-as-programming-language-featuring-ideas-from-lambda-calculus/overview/)

* [skastic: Visual programming language: SKetches of Abstract Syntax Trees. I. C.](https://github.com/mypalmike/skastic)

* [Mental Canvas: demo](https://www.reddit.com/r/nextfuckinglevel/comments/u2odk5/imagination_pushes_the_boundaries_of_space_and/)
	* https://www.mentalcanvas.com/

* [![](https://i.redd.it/p6r54w91ifw81.gif)](https://turtletoy.net/turtle/e1f58b05d7)

* https://www.louisbouchard.ai/editgan/
