use ash::Device;
use ash::version::{DeviceV1_0, V1_0};
use ash::vk;
use cgmath::Matrix4;
use std::ffi::CString;
use std::mem::size_of;
use std::ptr;
use std::rc::Rc;

use object::draw::Vertex;
use renderer::{RenderState, Texture};

use config::Config;

pub struct MainPass
{
	renderpass: vk::RenderPass,
	pub descriptor_pool: vk::DescriptorPool,
	pub descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
	pub pipeline_layout: vk::PipelineLayout,
	viewport: vk::Viewport,
	scissor: vk::Rect2D,
	pipeline: vk::Pipeline,
	// one framebuffer/commandbuffer per image
	framebuffer: vk::Framebuffer,
	commandbuffer: vk::CommandBuffer,

	// Image to render to.
	pub render_image: Texture,
	depth_image: Texture,

	view_matrix_ub: vk::Buffer,
	pub view_matrix_ub_mem: vk::DeviceMemory,
	view_matrix_ds: Vec<vk::DescriptorSet>,

	// Keep a pointer to the device for cleanup
	device: Rc<Device<V1_0>>,
}

impl MainPass
{
	/// Creates a main renderpass.
	fn create_renderpass(rs: &RenderState, render_format: vk::Format) -> vk::RenderPass
	{
		// One attachment, color only. Will produce the presentable image.
		let renderpass_attachments = [
			vk::AttachmentDescription {
				format: render_format,
				flags: vk::AttachmentDescriptionFlags::empty(),
				samples: vk::SAMPLE_COUNT_1_BIT,
				load_op: vk::AttachmentLoadOp::Clear,
				store_op: vk::AttachmentStoreOp::Store,
				stencil_load_op: vk::AttachmentLoadOp::DontCare,
				stencil_store_op: vk::AttachmentStoreOp::DontCare,
				initial_layout: vk::ImageLayout::ColorAttachmentOptimal,
				final_layout: vk::ImageLayout::ColorAttachmentOptimal,
			},
			vk::AttachmentDescription {
				format: vk::Format::D32Sfloat,
				flags: vk::AttachmentDescriptionFlags::empty(),
				samples: vk::SAMPLE_COUNT_1_BIT,
				load_op: vk::AttachmentLoadOp::Clear,
				store_op: vk::AttachmentStoreOp::DontCare,
				stencil_load_op: vk::AttachmentLoadOp::DontCare,
				stencil_store_op: vk::AttachmentStoreOp::DontCare,
				initial_layout: vk::ImageLayout::DepthStencilAttachmentOptimal,
				final_layout: vk::ImageLayout::DepthStencilAttachmentOptimal,
			},
		];
		let color_attachment_ref = vk::AttachmentReference {
			attachment: 0,
			layout: vk::ImageLayout::ColorAttachmentOptimal,
		};
		let depth_attachment_ref = vk::AttachmentReference {
			attachment: 1,
			layout: vk::ImageLayout::DepthStencilAttachmentOptimal,
		};
		let subpass = vk::SubpassDescription {
			color_attachment_count: 1,
			p_color_attachments: &color_attachment_ref,
			p_depth_stencil_attachment: &depth_attachment_ref,
			flags: Default::default(),
			pipeline_bind_point: vk::PipelineBindPoint::Graphics,
			input_attachment_count: 0,
			p_input_attachments: ptr::null(),
			p_resolve_attachments: ptr::null(),
			preserve_attachment_count: 0,
			p_preserve_attachments: ptr::null(),
		};
		let renderpass_create_info = vk::RenderPassCreateInfo {
			s_type: vk::StructureType::RenderPassCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			attachment_count: renderpass_attachments.len() as u32,
			p_attachments: renderpass_attachments.as_ptr(),
			subpass_count: 1,
			p_subpasses: &subpass,
			dependency_count: 0,
			p_dependencies: ptr::null(),
		};
		let renderpass;
		unsafe {
			renderpass = rs.device.create_render_pass(&renderpass_create_info, None).unwrap();
		}

		renderpass
	}

	/// Creates a pipeline for the renderpass.
	fn create_pipeline(
		rs: &RenderState, render_size: vk::Extent3D, renderpass: vk::RenderPass
	) -> (vk::DescriptorPool, Vec<vk::DescriptorSetLayout>, vk::PipelineLayout, vk::Viewport, vk::Rect2D, vk::Pipeline)
	{
		// Descriptors
		let descriptor_sizes = [
			vk::DescriptorPoolSize {
				typ: vk::DescriptorType::CombinedImageSampler,
				descriptor_count: 14,
			},
			vk::DescriptorPoolSize {
				typ: vk::DescriptorType::UniformBuffer,
				descriptor_count: 1,
			},
		];
		let descriptor_pool_info = vk::DescriptorPoolCreateInfo {
			s_type: vk::StructureType::DescriptorPoolCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			pool_size_count: descriptor_sizes.len() as u32,
			p_pool_sizes: descriptor_sizes.as_ptr(),
			max_sets: 8, // TODO figure out how to properly do this
		};
		let descriptor_pool;
		unsafe {
			descriptor_pool = rs.device.create_descriptor_pool(&descriptor_pool_info, None).unwrap();
		}
		let color_normal_tex_dsl_bindings = [
			vk::DescriptorSetLayoutBinding {
				binding: 0,
				descriptor_type: vk::DescriptorType::CombinedImageSampler,
				descriptor_count: 1,
				stage_flags: vk::SHADER_STAGE_FRAGMENT_BIT,
				p_immutable_samplers: ptr::null(),
			},
			vk::DescriptorSetLayoutBinding {
				binding: 1,
				descriptor_type: vk::DescriptorType::CombinedImageSampler,
				descriptor_count: 1,
				stage_flags: vk::SHADER_STAGE_FRAGMENT_BIT,
				p_immutable_samplers: ptr::null(),
			},
		];
		let view_matrix_dsl_binding = [
			vk::DescriptorSetLayoutBinding {
				binding: 0,
				descriptor_type: vk::DescriptorType::UniformBuffer,
				descriptor_count: 1,
				stage_flags: vk::SHADER_STAGE_VERTEX_BIT,
				p_immutable_samplers: ptr::null(),
			},
		];
		let color_normal_tex_info = vk::DescriptorSetLayoutCreateInfo {
			s_type: vk::StructureType::DescriptorSetLayoutCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			binding_count: color_normal_tex_dsl_bindings.len() as u32,
			p_bindings: color_normal_tex_dsl_bindings.as_ptr(),
		};
		let view_matrix_info = vk::DescriptorSetLayoutCreateInfo {
			s_type: vk::StructureType::DescriptorSetLayoutCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			binding_count: view_matrix_dsl_binding.len() as u32,
			p_bindings: view_matrix_dsl_binding.as_ptr(),
		};

		let descriptor_set_layouts;
		unsafe {
			descriptor_set_layouts = [
				rs.device.create_descriptor_set_layout(&color_normal_tex_info, None).unwrap(),
				rs.device.create_descriptor_set_layout(&view_matrix_info, None).unwrap(),
			];
		}

		let mv_matrices_push_constant = vk::PushConstantRange {
			stage_flags: vk::SHADER_STAGE_VERTEX_BIT,
			size: 2 * size_of::<Matrix4<f32>>() as u32,
			offset: 0,
		};

		let layout_create_info = vk::PipelineLayoutCreateInfo {
			s_type: vk::StructureType::PipelineLayoutCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			set_layout_count: descriptor_set_layouts.len() as u32,
			p_set_layouts: descriptor_set_layouts.as_ptr(),
			push_constant_range_count: 1,
			p_push_constant_ranges: &mv_matrices_push_constant,
		};

		let pipeline_layout;
		unsafe {
			pipeline_layout = rs.device.create_pipeline_layout(&layout_create_info, None).unwrap();
		}

		let vertex_shader_module = rs.load_shader("shaders/phong_vert.spv");
		let fragment_shader_module = rs.load_shader("shaders/phong_frag.spv");

		let shader_entry_name = CString::new("main").unwrap();
		let shader_stage_create_infos = [
			vk::PipelineShaderStageCreateInfo {
				s_type: vk::StructureType::PipelineShaderStageCreateInfo,
				p_next: ptr::null(),
				flags: Default::default(),
				module: vertex_shader_module,
				p_name: shader_entry_name.as_ptr(),
				p_specialization_info: ptr::null(),
				stage: vk::SHADER_STAGE_VERTEX_BIT,
			},
			vk::PipelineShaderStageCreateInfo {
				s_type: vk::StructureType::PipelineShaderStageCreateInfo,
				p_next: ptr::null(),
				flags: Default::default(),
				module: fragment_shader_module,
				p_name: shader_entry_name.as_ptr(),
				p_specialization_info: ptr::null(),
				stage: vk::SHADER_STAGE_FRAGMENT_BIT,
			},
		];

		// TODO: These would probably do better to live where the Vertex struct is defined.
		let vertex_binding_description = vk::VertexInputBindingDescription {
			binding: 0,
			stride: size_of::<Vertex>() as u32,
			input_rate: vk::VertexInputRate::Vertex,
		};

		let vertex_position_attribute_description = vk::VertexInputAttributeDescription {
			binding: 0,
			location: 0,
			format: vk::Format::R32g32b32Sfloat,
			offset: 0 as u32,
		};

		let vertex_normal_attribute_description = vk::VertexInputAttributeDescription {
			binding: 0,
			location: 1,
			format: vk::Format::R32g32b32Sfloat,
			offset: 3 * size_of::<f32>() as u32, // TODO: Make these use offset_of! macro.
		};

		let vertex_tangent_attribute_description = vk::VertexInputAttributeDescription {
			binding: 0,
			location: 2,
			format: vk::Format::R32g32b32Sfloat,
			offset: 6 * size_of::<f32>() as u32, // TODO: Make these use offset_of! macro.
		};

		let vertex_bitangent_attribute_description = vk::VertexInputAttributeDescription {
			binding: 0,
			location: 3,
			format: vk::Format::R32g32b32Sfloat,
			offset: 9 * size_of::<f32>() as u32, // TODO: Make these use offset_of! macro.
		};

		let vertex_texcoord_attribute_description = vk::VertexInputAttributeDescription {
			binding: 0,
			location: 4,
			format: vk::Format::R32g32Sfloat,
			offset: 12 * size_of::<f32>() as u32, // TODO: Make these use offset_of! macro.
		};

		let vertex_input_binding_descriptions = [vertex_binding_description];
		let vertex_input_attribute_descriptions = [
			vertex_position_attribute_description,
			vertex_normal_attribute_description,
			vertex_tangent_attribute_description,
			vertex_bitangent_attribute_description,
			vertex_texcoord_attribute_description,
		];
		let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo {
			s_type: vk::StructureType::PipelineVertexInputStateCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			vertex_attribute_description_count: vertex_input_attribute_descriptions.len() as u32,
			p_vertex_attribute_descriptions: vertex_input_attribute_descriptions.as_ptr(),
			vertex_binding_description_count: vertex_input_binding_descriptions.len() as u32,
			p_vertex_binding_descriptions: vertex_input_binding_descriptions.as_ptr(),
		};
		let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
			s_type: vk::StructureType::PipelineInputAssemblyStateCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			primitive_restart_enable: 0,
			topology: vk::PrimitiveTopology::TriangleList,
		};
		let viewport = vk::Viewport {
			x: 0.0,
			y: 0.0,
			width: render_size.width as f32,
			height: render_size.height as f32,
			min_depth: 0.0,
			max_depth: 1.0,
		};
		let scissor = vk::Rect2D {
			offset: vk::Offset2D {
				x: 0,
				y: 0,
			},
			extent: vk::Extent2D {
				width: render_size.width,
				height: render_size.height,
			},
		};
		let viewport_state_info = vk::PipelineViewportStateCreateInfo {
			s_type: vk::StructureType::PipelineViewportStateCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			scissor_count: 1,
			p_scissors: &scissor,
			viewport_count: 1,
			p_viewports: &viewport,
		};
		let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
			s_type: vk::StructureType::PipelineRasterizationStateCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			cull_mode: vk::CULL_MODE_BACK_BIT,
			depth_bias_clamp: 0.0,
			depth_bias_constant_factor: 0.0,
			depth_bias_enable: 0,
			depth_bias_slope_factor: 0.0,
			depth_clamp_enable: 0,
			front_face: vk::FrontFace::CounterClockwise,
			line_width: 1.0,
			polygon_mode: vk::PolygonMode::Fill,
			rasterizer_discard_enable: 0,
		};
		let multisample_state_info = vk::PipelineMultisampleStateCreateInfo {
			s_type: vk::StructureType::PipelineMultisampleStateCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			rasterization_samples: vk::SAMPLE_COUNT_1_BIT,
			sample_shading_enable: 0,
			min_sample_shading: 0.0,
			p_sample_mask: ptr::null(),
			alpha_to_one_enable: 0,
			alpha_to_coverage_enable: 0,
		};
		let noop_stencil_state = vk::StencilOpState {
			fail_op: vk::StencilOp::Keep,
			pass_op: vk::StencilOp::Keep,
			depth_fail_op: vk::StencilOp::Keep,
			compare_op: vk::CompareOp::Always,
			compare_mask: 0,
			write_mask: 0,
			reference: 0,
		};
		let depth_state_info = vk::PipelineDepthStencilStateCreateInfo {
			s_type: vk::StructureType::PipelineDepthStencilStateCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			depth_test_enable: 1,
			depth_write_enable: 1,
			depth_compare_op: vk::CompareOp::LessOrEqual,
			depth_bounds_test_enable: 0,
			stencil_test_enable: 0,
			front: noop_stencil_state.clone(),
			back: noop_stencil_state.clone(),
			max_depth_bounds: 1.0,
			min_depth_bounds: 0.0,
		};
		let color_blend_attachment_states = [
			vk::PipelineColorBlendAttachmentState {
				blend_enable: 0,
				src_color_blend_factor: vk::BlendFactor::SrcColor,
				dst_color_blend_factor: vk::BlendFactor::OneMinusDstColor,
				color_blend_op: vk::BlendOp::Add,
				src_alpha_blend_factor: vk::BlendFactor::Zero,
				dst_alpha_blend_factor: vk::BlendFactor::Zero,
				alpha_blend_op: vk::BlendOp::Add,
				color_write_mask: vk::ColorComponentFlags::all(),
			},
		];
		let color_blend_state = vk::PipelineColorBlendStateCreateInfo {
			s_type: vk::StructureType::PipelineColorBlendStateCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			logic_op_enable: 0,
			logic_op: vk::LogicOp::Clear,
			attachment_count: color_blend_attachment_states.len() as u32,
			p_attachments: color_blend_attachment_states.as_ptr(),
			blend_constants: [0.0, 0.0, 0.0, 0.0],
		};
		let dynamic_state = [vk::DynamicState::Viewport, vk::DynamicState::Scissor];
		let dynamic_state_info = vk::PipelineDynamicStateCreateInfo {
			s_type: vk::StructureType::PipelineDynamicStateCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			dynamic_state_count: dynamic_state.len() as u32,
			p_dynamic_states: dynamic_state.as_ptr(),
		};
		let graphic_pipeline_info = vk::GraphicsPipelineCreateInfo {
			s_type: vk::StructureType::GraphicsPipelineCreateInfo,
			p_next: ptr::null(),
			flags: vk::PipelineCreateFlags::empty(),
			stage_count: shader_stage_create_infos.len() as u32,
			p_stages: shader_stage_create_infos.as_ptr(),
			p_vertex_input_state: &vertex_input_state_info,
			p_input_assembly_state: &vertex_input_assembly_state_info,
			p_tessellation_state: ptr::null(),
			p_viewport_state: &viewport_state_info,
			p_rasterization_state: &rasterization_info,
			p_multisample_state: &multisample_state_info,
			p_depth_stencil_state: &depth_state_info,
			p_color_blend_state: &color_blend_state,
			p_dynamic_state: &dynamic_state_info,
			layout: pipeline_layout,
			render_pass: renderpass,
			subpass: 0,
			base_pipeline_handle: vk::Pipeline::null(),
			base_pipeline_index: 0,
		};
		let graphics_pipelines;
		unsafe {
			graphics_pipelines = rs.device
				.create_graphics_pipelines(vk::PipelineCache::null(), &[graphic_pipeline_info], None)
				.expect("Unable to create graphics pipeline");

			// Graphics pipeline created, we no longer need the shader modules
			rs.device.destroy_shader_module(fragment_shader_module, None);
			rs.device.destroy_shader_module(vertex_shader_module, None);
		}

		(descriptor_pool, descriptor_set_layouts.to_vec(), pipeline_layout, viewport, scissor, graphics_pipelines[0])
	}

	/// Creates framebuffers for the presentable images, one per image.
	fn create_framebuffer(
		rs: &RenderState, render_size: vk::Extent3D, color_view: vk::ImageView, depth_view: vk::ImageView,
		renderpass: vk::RenderPass,
	) -> vk::Framebuffer
	{
		let framebuffer_attachments = [color_view, depth_view];
		let frame_buffer_create_info = vk::FramebufferCreateInfo {
			s_type: vk::StructureType::FramebufferCreateInfo,
			p_next: ptr::null(),
			flags: Default::default(),
			render_pass: renderpass,
			attachment_count: framebuffer_attachments.len() as u32,
			p_attachments: framebuffer_attachments.as_ptr(),
			width: render_size.width,
			height: render_size.height,
			layers: 1,
		};
		let framebuffer;
		unsafe {
			framebuffer = rs.device.create_framebuffer(&frame_buffer_create_info, None).unwrap();
		}
		framebuffer
	}

	/// Creates commandbuffer.
	fn create_commandbuffer(rs: &RenderState) -> vk::CommandBuffer
	{
		let command_buffer_allocate_info = vk::CommandBufferAllocateInfo {
			s_type: vk::StructureType::CommandBufferAllocateInfo,
			p_next: ptr::null(),
			command_buffer_count: 1,
			command_pool: rs.commandpool,
			level: vk::CommandBufferLevel::Primary,
		};
		let commandbuffers;
		unsafe {
			commandbuffers = rs.device.allocate_command_buffers(&command_buffer_allocate_info).unwrap();
		}

		commandbuffers[0]
	}

	/// Initializes the MainPass based on a RenderState
	///
	/// This will set up the renderpass, etc.
	pub fn init(rs: &RenderState, cfg: &Config) -> MainPass
	{
		let render_format = vk::Format::R8g8b8a8Unorm;
		let render_size = vk::Extent3D {
			width: cfg.render_width,
			height: cfg.render_height,
			depth: 1,
		};

		// Create image to render to.
		let render_image = rs.create_texture(
			render_size,
			vk::ImageType::Type2d,
			vk::ImageViewType::Type2d,
			render_format,
			vk::IMAGE_ASPECT_COLOR_BIT,
			vk::IMAGE_USAGE_COLOR_ATTACHMENT_BIT | vk::IMAGE_USAGE_SAMPLED_BIT,
			vk::ACCESS_COLOR_ATTACHMENT_READ_BIT | vk::ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
			vk::ImageLayout::ColorAttachmentOptimal,
			vk::PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
			None,
		);
		let depth_image = rs.create_texture(
			render_size,
			vk::ImageType::Type2d,
			vk::ImageViewType::Type2d,
			vk::Format::D32Sfloat,
			vk::IMAGE_ASPECT_DEPTH_BIT,
			vk::IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT,
			vk::ACCESS_DEPTH_STENCIL_ATTACHMENT_READ_BIT | vk::ACCESS_DEPTH_STENCIL_ATTACHMENT_WRITE_BIT,
			vk::ImageLayout::DepthStencilAttachmentOptimal,
			vk::PIPELINE_STAGE_ALL_GRAPHICS_BIT,
			None,
		);

		let renderpass = MainPass::create_renderpass(rs, render_format);
		let (descriptor_pool, descriptor_set_layouts, pipeline_layout, viewport, scissor, pipeline) =
			MainPass::create_pipeline(rs, render_size, renderpass);
		let framebuffer =
			MainPass::create_framebuffer(rs, render_size, render_image.view, depth_image.view, renderpass);
		let commandbuffer = MainPass::create_commandbuffer(rs);

		let (vmat_buf, vmat_mem) = rs.create_buffer(
			vk::BUFFER_USAGE_UNIFORM_BUFFER_BIT,
			vk::MEMORY_PROPERTY_HOST_VISIBLE_BIT | vk::MEMORY_PROPERTY_HOST_COHERENT_BIT,
			size_of::<Matrix4<f32>>() as u64,
		);
		let desc_alloc_info = vk::DescriptorSetAllocateInfo {
			s_type: vk::StructureType::DescriptorSetAllocateInfo,
			p_next: ptr::null(),
			descriptor_pool: descriptor_pool,
			descriptor_set_count: 1,
			p_set_layouts: &descriptor_set_layouts[1],
		};
		let view_matrix_ds;
		unsafe {
			view_matrix_ds = rs.device.allocate_descriptor_sets(&desc_alloc_info).unwrap();
		}

		MainPass {
			renderpass: renderpass,
			descriptor_pool: descriptor_pool,
			descriptor_set_layouts: descriptor_set_layouts,
			pipeline_layout: pipeline_layout,
			viewport: viewport,
			scissor: scissor,
			pipeline: pipeline,
			framebuffer: framebuffer,
			commandbuffer: commandbuffer,

			render_image: render_image,
			depth_image: depth_image,

			view_matrix_ub: vmat_buf,
			view_matrix_ub_mem: vmat_mem,
			view_matrix_ds: view_matrix_ds,

			// Keep a pointer to the device for cleanup
			device: Rc::clone(&rs.device),
		}
	}
	/// Begins the main render pass
	///
	/// Returns a command buffer to be used in rendering.
	pub fn begin_frame(&mut self, rs: &RenderState) -> vk::CommandBuffer
	{
		// Begin commandbuffer
		let cmd_buf_begin_info = vk::CommandBufferBeginInfo {
			s_type: vk::StructureType::CommandBufferBeginInfo,
			p_next: ptr::null(),
			p_inheritance_info: ptr::null(),
			flags: vk::COMMAND_BUFFER_USAGE_SIMULTANEOUS_USE_BIT,
		};
		let cmd_buf = self.commandbuffer;
		unsafe {
			rs.device.begin_command_buffer(cmd_buf, &cmd_buf_begin_info).expect("Begin commandbuffer");
		}

		// Transition the mainpass output to a renderable image
		rs.transition_texture(
			&mut self.render_image,
			vk::ACCESS_COLOR_ATTACHMENT_READ_BIT | vk::ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
			vk::ImageLayout::ColorAttachmentOptimal,
			vk::PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
			Some(cmd_buf),
		);

		// Begin renderpass
		let clear_values = [
			vk::ClearValue::new_color(vk::ClearColorValue::new_float32([0.0, 1.0, 0.0, 1.0])),
			vk::ClearValue::new_depth_stencil(vk::ClearDepthStencilValue {
				depth: 1.0,
				stencil: 0,
			}),
		];

		let render_pass_begin_info = vk::RenderPassBeginInfo {
			s_type: vk::StructureType::RenderPassBeginInfo,
			p_next: ptr::null(),
			render_pass: self.renderpass,
			framebuffer: self.framebuffer,
			render_area: self.scissor,
			clear_value_count: clear_values.len() as u32,
			p_clear_values: clear_values.as_ptr(),
		};

		let view_matrix_ub_descriptor = vk::DescriptorBufferInfo {
			buffer: self.view_matrix_ub,
			offset: 0,
			range: size_of::<Matrix4<f32>>() as u64,
		};
		let write_desc_sets = [
			vk::WriteDescriptorSet {
				s_type: vk::StructureType::WriteDescriptorSet,
				p_next: ptr::null(),
				dst_set: self.view_matrix_ds[0],
				dst_binding: 0,
				dst_array_element: 0,
				descriptor_count: 1,
				descriptor_type: vk::DescriptorType::UniformBuffer,
				p_image_info: ptr::null(),
				p_buffer_info: &view_matrix_ub_descriptor,
				p_texel_buffer_view: ptr::null(),
			},
		];

		unsafe {
			// Update the view matrix descriptor set
			rs.device.update_descriptor_sets(&write_desc_sets, &[]);

			// Start the render pass
			rs.device.cmd_begin_render_pass(cmd_buf, &render_pass_begin_info, vk::SubpassContents::Inline);

			rs.device.cmd_bind_descriptor_sets(
				cmd_buf,
				vk::PipelineBindPoint::Graphics,
				self.pipeline_layout,
				1,
				&self.view_matrix_ds[..],
				&[],
			);

			// Bind pipeline
			rs.device.cmd_bind_pipeline(cmd_buf, vk::PipelineBindPoint::Graphics, self.pipeline);

			rs.device.cmd_set_viewport(cmd_buf, &[self.viewport]);
			rs.device.cmd_set_scissor(cmd_buf, &[self.scissor]);
		}

		cmd_buf
	}

	/// Ends the main render frame
	pub fn end_frame(&mut self, rs: &RenderState)
	{
		let cmd_buf = self.commandbuffer;

		unsafe {
			// End render pass and command buffer
			rs.device.cmd_end_render_pass(cmd_buf);
			rs.device.end_command_buffer(cmd_buf).expect("End commandbuffer");
		}

		// Send the work off to the GPU
		let submit_info = vk::SubmitInfo {
			s_type: vk::StructureType::SubmitInfo,
			p_next: ptr::null(),
			wait_semaphore_count: 0,
			p_wait_semaphores: ptr::null(),
			p_wait_dst_stage_mask: ptr::null(),
			command_buffer_count: 1,
			p_command_buffers: &cmd_buf,
			signal_semaphore_count: 0,
			p_signal_semaphores: ptr::null(),
		};
		unsafe {
			rs.device.queue_submit(rs.graphics_queue, &[submit_info], vk::Fence::null()).expect("queue submit failed.");
		}
	}
}

impl Drop for MainPass
{
	fn drop(&mut self)
	{
		// We cannot have the last reference to device at this point
		debug_assert!(1 < Rc::strong_count(&self.device));

		unsafe {
			// Always wait for device idle
			self.device.device_wait_idle().unwrap();

			self.device.destroy_buffer(self.view_matrix_ub, None);
			self.device.free_memory(self.view_matrix_ub_mem, None);

			self.device.destroy_sampler(self.depth_image.sampler, None);
			self.device.destroy_image_view(self.depth_image.view, None);
			self.device.destroy_image(self.depth_image.image, None);
			self.device.free_memory(self.depth_image.memory, None);

			self.device.destroy_sampler(self.render_image.sampler, None);
			self.device.destroy_image_view(self.render_image.view, None);
			self.device.destroy_image(self.render_image.image, None);
			self.device.free_memory(self.render_image.memory, None);

			self.device.destroy_framebuffer(self.framebuffer, None);

			self.device.destroy_pipeline(self.pipeline, None);
			self.device.destroy_pipeline_layout(self.pipeline_layout, None);

			for &dset_layout in self.descriptor_set_layouts.iter()
			{
				self.device.destroy_descriptor_set_layout(dset_layout, None);
			}

			self.device.destroy_descriptor_pool(self.descriptor_pool, None);

			self.device.destroy_render_pass(self.renderpass, None);
		}
	}
}
