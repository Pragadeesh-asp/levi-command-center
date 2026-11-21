import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

export default function MarkdownMessage({content}:{content:string}) {
  return <div className="ai-markdown">
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        a:({href,children})=><span className="ai-markdown-link" title={href}>{children}{href&&<small>{href}</small>}</span>,
      }}
    >{content}</ReactMarkdown>
  </div>
}
