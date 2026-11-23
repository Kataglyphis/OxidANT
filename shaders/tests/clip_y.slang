// The clip y of each target's top image row, written apart from fullscreen.slang so a swapped target branch fails a kernel.
float top_row_clip_y()
{
    __target_switch
    {
    case spirv:
        return -1.0;
    default:
        return 1.0;
    }
}
