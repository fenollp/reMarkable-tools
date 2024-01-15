pub mod proto {
    pub mod hypercards {
        // subl $(ls -t target/*/*/build/marauder-*/out/*.rs|head -n1)
        tonic::include_proto!("hypercards");
    }
}
