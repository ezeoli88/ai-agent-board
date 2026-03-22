import type { ChatMessageEvent } from '@dash-agent/shared'

export function ChatMessageBubble({ message }: { message: ChatMessageEvent }) {
  if (message.role === 'system') {
    return (
      <div className="flex justify-center py-1.5">
        <span className="text-xs text-zinc-400 dark:text-zinc-500 italic">
          {message.content}
        </span>
      </div>
    )
  }

  if (message.role === 'user') {
    return (
      <div className="flex justify-end py-1.5">
        <div className="max-w-[85%] rounded-lg px-3 py-2 bg-blue-600 dark:bg-blue-500 text-white dark:text-white">
          <p className="text-sm whitespace-pre-wrap break-words">{message.content}</p>
        </div>
      </div>
    )
  }

  // assistant
  return (
    <div className="flex justify-start py-1.5">
      <div className="max-w-[85%] rounded-lg px-3 py-2 bg-zinc-800 dark:bg-zinc-200 text-zinc-100 dark:text-zinc-900">
        <p className="text-sm whitespace-pre-wrap break-words">{message.content}</p>
      </div>
    </div>
  )
}
