pub mod modes;
pub mod shapes;
pub mod strokes;

pub mod unipen;

pub mod drawings;

pub mod proto {
    pub mod whiteboard {
        // subl $(ls -t target/*/*/build/marauder-*/out/hypercard.whiteboard.rs|head -n1)
        tonic::include_proto!("hypercard.whiteboard");
    }
}
