#version 300 es

#extension GL_OES_EGL_image_external : require

precision highp float;

in vec2 v_texcoord;
out vec4 outColor;

uniform samplerExternalOES tex;
uniform float width;
uniform float height;

// Sample from https://learnopengl.com/Advanced-OpenGL/Framebuffers
void main()
{
    float offset_x = 1.0 / width;
    float offset_y = 1.0 / height;

    vec2 offsets[9] = vec2[](
        vec2(-offset_x,  offset_y), // top-left
        vec2( 0.0f,    offset_y), // top-center
        vec2( offset_x,  offset_y), // top-right
        vec2(-offset_x,  0.0f),   // center-left
        vec2( 0.0f,    0.0f),   // center-center
        vec2( offset_x,  0.0f),   // center-right
        vec2(-offset_x, -offset_y), // bottom-left
        vec2( 0.0f,   -offset_y), // bottom-center
        vec2( offset_x, -offset_y)  // bottom-right    
    );

    float kernel[9] = float[](
        1.0, 2.0, 1.0,
        0.0, 0.0, 0.0,
        -1.0, -2.0, -1.0
    );

    vec3 col = vec3(0.0);
    for(int i = 0; i < 9; i++)
    {
        vec3 c = texture2D(tex, v_texcoord.xy + offsets[i]).rgb;
        col += c * kernel[i];
    }

    outColor = vec4(col, 1.0);
}