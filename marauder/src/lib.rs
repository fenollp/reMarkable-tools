pub mod fonts;
pub mod modes;
pub mod shapes;
pub mod strokes;

pub mod unipen;

pub mod drawings;

pub mod proto {
    pub mod hypercards {
        // subl $(ls -t target/*/*/build/marauder-*/out/*.rs|head -n1)
        tonic::include_proto!("hypercards");
    }
}
