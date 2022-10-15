// use realfft::RealFftPlanner;
// use rustfft::num_complex::Complex;
// use rustfft::num_traits::Zero;

// pub fn convert_pcm(data: &[i32], sample_rate: i32) {
//     let length = data.len();

//     // make a planner
//     let mut real_planner = RealFftPlanner::<i32>::new();

//     // create a FFT
//     let r2c = real_planner.plan_fft_forward(length);
//     // make input and output vectors
//     let mut indata = r2c.make_input_vec();
//     let mut spectrum = r2c.make_output_vec();

//     // Are they the length we expect?
//     // Forward transform the input data
//     r2c.process(&mut indata, &mut spectrum).unwrap();

//     // create an iFFT and an output vector
//     let c2r = real_planner.plan_fft_inverse(length);
//     let mut outdata = c2r.make_output_vec();
//     assert_eq!(outdata.len(), length);

//     c2r.process(&mut spectrum, &mut outdata).unwrap();

//     println!("{:?}", spectrum);
//     println!("{:?}", outdata);
// }
