document.addEventListener('DOMContentLoaded', () => {
    // Add subtle parallax to background blobs
    document.addEventListener('mousemove', (e) => {
        const x = e.clientX / window.innerWidth;
        const y = e.clientY / window.innerHeight;
        
        const blobs = document.querySelectorAll('.blob');
        blobs.forEach((blob, index) => {
            const factor = index === 0 ? 30 : -30;
            blob.style.transform = `translate(${x * factor}px, ${y * factor}px)`;
        });
    });
});
