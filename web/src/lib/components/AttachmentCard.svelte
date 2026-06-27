<script lang="ts">
	import { type AttachmentItem, getAttachmentDownloadUrl } from '$lib/api';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';

	interface Props {
		attachment: AttachmentItem;
	}

	let { attachment }: Props = $props();

	let downloadUrl = $derived(getAttachmentDownloadUrl(attachment.id));

	let isImage = $derived(
		attachment.content_type ? attachment.content_type.startsWith('image/') : false
	);

	let isText = $derived(
		attachment.content_type ? attachment.content_type.startsWith('text/') : false
	);

	let fileIcon = $derived(isImage ? 'image' : isText ? 'text' : 'archive');

	let truncatedFilename = $derived(
		attachment.filename.length > 30
			? attachment.filename.slice(0, 27) + '...'
			: attachment.filename
	);

	function formatFileSize(bytes: number): string {
		if (bytes < 1024) return bytes + ' B';
		if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
		return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
	}

	let displaySize = $derived(formatFileSize(attachment.size_bytes));

	function handleImgError(e: Event) {
		const img = e.target as HTMLImageElement;
		img.style.display = 'none';
	}
</script>

<div class="ring-foreground/10 bg-card text-card-foreground overflow-hidden rounded-xl text-sm shadow-xs ring-1 flex flex-col">
	{#if isImage}
		<a href={downloadUrl} target="_blank" rel="noopener noreferrer" class="block">
			<img
				src={downloadUrl}
				alt={attachment.filename}
				loading="lazy"
				class="w-full h-40 object-cover bg-muted"
				onerror={handleImgError}
			/>
		</a>
	{/if}

	<div class="p-4 space-y-2">
		<div class="flex items-start justify-between gap-2">
			<div class="flex items-center gap-2 min-w-0">
				<span class="text-lg leading-none shrink-0" aria-hidden="true">
					{#if fileIcon === 'image'}&#x1F5BC;&#xFE0F;
					{:else if fileIcon === 'text'}&#x1F4C4;
					{:else}&#x1F4E6;{/if}
				</span>
				<span class="text-sm font-medium truncate" title={attachment.filename}>
					{truncatedFilename}
				</span>
			</div>
		</div>

		<div class="flex items-center justify-between">
			<span class="text-xs text-muted-foreground">{displaySize}</span>
			{#if attachment.content_type}
				<Badge variant="outline" class="text-[10px] px-1.5 py-0">
					{attachment.content_type}
				</Badge>
			{/if}
		</div>

		<Button variant="outline" size="sm" class="w-full mt-1" href={downloadUrl} download={attachment.filename}>
			&#x2B07; Download
		</Button>
	</div>
</div>
